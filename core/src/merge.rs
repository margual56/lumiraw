//! Combining a bracket of exposures into one frame that holds all of them.

use crate::decode::{self, Meta};
use crate::ops::{self, Image};

/// One exposure of the bracket, decoded but otherwise untouched.
pub struct Frame {
    pub name: String,
    pub linear: Image,
    pub meta: Meta,
    /// The tags this frame was shot with.
    pub exif: Vec<crate::exif::Entry>,
}

impl Frame {
    pub fn open(name: &str, bytes: &[u8]) -> Result<Frame, String> {
        let decoded = decode::decode(bytes)?;
        Ok(Frame { name: name.to_string(), linear: decoded.img, meta: decoded.meta,
                   exif: decode::collect_exif(bytes) })
    }

    /// How much light this frame was given, in arbitrary but consistent units.
    pub fn exposure(&self) -> f32 {
        let aperture = self.meta.aperture.max(0.5);
        (self.meta.shutter_s.max(1e-6) * self.meta.iso.max(1.0) / (aperture * aperture)).max(1e-12)
    }

    /// Where this frame sits in the bracket, in stops relative to another.
    pub fn stops_from(&self, other: &Frame) -> f32 {
        (self.exposure() / other.exposure()).log2()
    }
}

/// How the merge should behave.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Line the frames up before merging. Worth it handheld, harmless on a
    /// tripod, and the cost is a few pyramid levels of bit counting.
    pub align: bool,
    /// How eagerly to reject pixels where the scene itself moved.
    pub deghost: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options { align: true, deghost: 0.5 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Notes {
    pub reference: usize,
    pub stops: Vec<f32>,
    pub shifts: Vec<(i32, i32)>,
    /// Fraction of the frame where the exposures disagreed enough to be
    /// treated as movement.
    pub ghosted: f32,
    /// Fraction of the frame no exposure captured well: blown in the darkest
    /// frame, or lost in the noise of the brightest.
    pub uncovered: f32,
    pub range_stops: f32,
}

/// Above this share of the compared pixels still disagreeing after the best
/// shift, the two frames are not showing the same thing.
const NO_CORRESPONDENCE: f32 = 0.22;

// A frame's reading is worth having when it is clear of the noise at the bottom
// and clear of the ceiling at the top.
const NOISE_FLOOR: f32 = 0.0015;
const TRUSTED_LOW: f32 = 0.02;
const TRUSTED_HIGH: f32 = 0.80;
const CEILING: f32 = 0.97;

/// How much a reading of this level deserves to be believed.
fn reliability(level: f32) -> f32 {
    ops::smoothstep(NOISE_FLOOR, TRUSTED_LOW, level)
        * (1.0 - ops::smoothstep(TRUSTED_HIGH, CEILING, level))
}

/// Merge a bracket into one scene-linear frame.
pub fn merge(frames: &[Frame], options: Options) -> Result<(Image, Notes), String> {
    if frames.len() < 2 {
        return Err("a merge needs at least two exposures".into());
    }
    let (w, h) = (frames[0].linear.w, frames[0].linear.h);
    if frames.iter().any(|f| f.linear.w != w || f.linear.h != h) {
        return Err("the exposures are different sizes, so they are not the same shot".into());
    }

    // The middle exposure is the reference: it is the one most likely to hold
    // the subject at a sensible level, and the one a viewer would recognise.
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by(|a, b| {
        frames[*a].exposure().partial_cmp(&frames[*b].exposure()).unwrap()
    });
    let reference = order[order.len() / 2];

    let mut notes = Notes {
        reference,
        stops: frames.iter().map(|f| f.stops_from(&frames[reference])).collect(),
        shifts: vec![(0, 0); frames.len()],
        ..Notes::default()
    };
    notes.range_stops = notes.stops.iter().cloned().fold(f32::MIN, f32::max)
        - notes.stops.iter().cloned().fold(f32::MAX, f32::min);

    if options.align {
        let anchor = median_bitmaps(&frames[reference].linear);
        for (i, frame) in frames.iter().enumerate() {
            if i != reference {
                notes.shifts[i] = align(&anchor, &median_bitmaps(&frame.linear));
            }
        }
    }

    // Everything is expressed in the reference frame's light, so a frame given
    // twice the exposure contributes half the value for the same radiance.
    let scale: Vec<f32> = frames
        .iter()
        .map(|f| frames[reference].exposure() / f.exposure())
        .collect();

    // A frame given more light is a better measurement of the same radiance,
    // and not by a little.
    let brightest = scale.iter().cloned().fold(f32::MAX, f32::min);
    let snr: Vec<f32> = scale.iter().map(|s| brightest / s).collect();

    let n = w * h;
    let mut sum = vec![0f32; n * 3];
    let mut weight = vec![0f32; n];
    // Kept for the pixels nothing measured well, and for deghosting.
    let mut fallback = vec![0f32; n * 3];
    let mut fallback_score = vec![f32::MIN; n];

    for (j, frame) in frames.iter().enumerate() {
        let (dx, dy) = notes.shifts[j];
        for y in 0..h {
            let sy = y as i32 + dy;
            if sy < 0 || sy >= h as i32 {
                continue;
            }
            for x in 0..w {
                let sx = x as i32 + dx;
                if sx < 0 || sx >= w as i32 {
                    continue;
                }
                let src = ((sy as usize) * w + sx as usize) * 3;
                let dst = (y * w + x) * 3;
                let px = [frame.linear.d[src], frame.linear.d[src + 1], frame.linear.d[src + 2]];
                let level = px[0].max(px[1]).max(px[2]);
                let trust = reliability(level) * snr[j];
                if trust > 0.0 {
                    weight[y * w + x] += trust;
                    for c in 0..3 {
                        sum[dst + c] += px[c] * scale[j] * trust;
                    }
                }
                // Whatever happens, remember the frame that came closest to a
                // usable reading, so no pixel is left without a value.
                let score = -(level - 0.35).abs();
                if score > fallback_score[y * w + x] {
                    fallback_score[y * w + x] = score;
                    for c in 0..3 {
                        fallback[dst + c] = px[c] * scale[j];
                    }
                }
            }
        }
    }

    let mut out = Image::new(w, h);
    let mut uncovered = 0usize;
    for i in 0..n {
        if weight[i] > 1e-4 {
            for c in 0..3 {
                out.d[i * 3 + c] = sum[i * 3 + c] / weight[i];
            }
        } else {
            uncovered += 1;
            for c in 0..3 {
                out.d[i * 3 + c] = fallback[i * 3 + c];
            }
        }
    }
    notes.uncovered = uncovered as f32 / n as f32;

    if options.deghost > 0.0 {
        notes.ghosted = deghost(&mut out, frames, &scale, &notes.shifts, reference, options.deghost);
    }
    Ok((out, notes))
}

/// Replace pixels where the frames disagree about the scene with the reference
/// frame's own reading.
fn deghost(out: &mut Image, frames: &[Frame], scale: &[f32], shifts: &[(i32, i32)],
           reference: usize, amount: f32) -> f32 {
    let (w, h) = (out.w, out.h);
    let (dx, dy) = shifts[reference];
    // A generous threshold at amount 0, tightening as the control rises.
    let tolerance = 0.9 - 0.7 * amount.clamp(0.0, 1.0);
    let mut moved = 0usize;

    for y in 0..h {
        let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
        for x in 0..w {
            let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
            let src = (sy * w + sx) * 3;
            let dst = (y * w + x) * 3;
            let px = [frames[reference].linear.d[src], frames[reference].linear.d[src + 1],
                      frames[reference].linear.d[src + 2]];
            let level = px[0].max(px[1]).max(px[2]);
            // The reference can only arbitrate where it saw something itself.
            if reliability(level) <= 0.0 {
                continue;
            }
            let theirs = [out.d[dst], out.d[dst + 1], out.d[dst + 2]];
            let mine = [px[0] * scale[reference], px[1] * scale[reference],
                        px[2] * scale[reference]];
            let a = mine[0].max(mine[1]).max(mine[2]).max(1e-6);
            let b = theirs[0].max(theirs[1]).max(theirs[2]).max(1e-6);
            // Compare as a ratio: a disagreement matters in proportion to how
            // much light is there, not in absolute terms.
            let disagreement = (a / b).log2().abs();
            if disagreement > tolerance {
                moved += 1;
                for c in 0..3 {
                    out.d[dst + c] = mine[c];
                }
            }
        }
    }
    moved as f32 / (w * h) as f32
}

// -------------------------------------------------------------------------
// alignment
// -------------------------------------------------------------------------

/// A frame reduced to what survives a change of exposure.
struct Bitmaps {
    levels: Vec<(Vec<bool>, Vec<bool>, usize, usize)>, // above median, worth counting, w, h
}

/// Threshold a frame at its own median.
fn median_bitmaps(img: &Image) -> Bitmaps {
    let mut grey = ops::luminance(img);
    // Work in a perceptual-ish scale so the median sits somewhere useful.
    for v in grey.d.iter_mut() {
        *v = v.max(0.0).sqrt();
    }
    let mut levels = Vec::new();
    let mut plane = grey;
    while levels.len() < 6 && plane.w > 32 && plane.h > 32 {
        let mut sorted = plane.d.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        let tolerance = (sorted[sorted.len() * 3 / 4] - sorted[sorted.len() / 4]) * 0.02;
        let above: Vec<bool> = plane.d.iter().map(|v| *v > median).collect();
        let usable: Vec<bool> =
            plane.d.iter().map(|v| (*v - median).abs() > tolerance).collect();
        levels.push((above, usable, plane.w, plane.h));
        plane = ops::resize_plane(&plane, plane.w / 2, plane.h / 2);
    }
    Bitmaps { levels }
}

/// The shift that best lines up two frames, found coarse to fine.
fn align(anchor: &Bitmaps, other: &Bitmaps) -> (i32, i32) {
    let depth = anchor.levels.len().min(other.levels.len());
    if depth == 0 {
        return (0, 0);
    }
    let mut shift = (0i32, 0i32);
    for level in (0..depth).rev() {
        shift = (shift.0 * 2, shift.1 * 2);
        let (ref_bits, ref_use, w, h) = &anchor.levels[level];
        let (bits, use_bits, ow, oh) = &other.levels[level];
        if w != ow || h != oh {
            continue;
        }
        let mut best = shift;
        let mut best_error = usize::MAX;
        let mut best_compared = 0usize;
        // Nine candidates: the inherited shift and one pixel around it.
        for dy in -1..=1 {
            for dx in -1..=1 {
                let candidate = (shift.0 + dx, shift.1 + dy);
                let (error, compared) =
                    mismatch(ref_bits, ref_use, bits, use_bits, *w, *h, candidate);
                if error < best_error {
                    best_error = error;
                    best_compared = compared;
                    best = candidate;
                }
            }
        }
        shift = best;
        // At the finest level, ask whether any of this was worth doing.
        if level == 0 {
            let disagreement = if best_compared == 0 {
                1.0
            } else {
                best_error as f32 / best_compared as f32
            };
            if disagreement > NO_CORRESPONDENCE {
                return (0, 0);
            }
        }
    }
    shift
}

/// How many pixels the two bitmaps disagree about at this offset.
fn mismatch(a: &[bool], a_use: &[bool], b: &[bool], b_use: &[bool], w: usize, h: usize,
            (dx, dy): (i32, i32)) -> (usize, usize) {
    let mut wrong = 0usize;
    let mut compared = 0usize;
    for y in 0..h {
        let sy = y as i32 + dy;
        if sy < 0 || sy >= h as i32 {
            continue;
        }
        for x in 0..w {
            let sx = x as i32 + dx;
            if sx < 0 || sx >= w as i32 {
                continue;
            }
            let i = y * w + x;
            let j = sy as usize * w + sx as usize;
            if a_use[i] && b_use[j] {
                compared += 1;
                if a[i] != b[j] {
                    wrong += 1;
                }
            }
        }
    }
    (wrong, compared)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(shutter: f32, iso: f32) -> Meta {
        Meta { shutter_s: shutter, iso, aperture: 5.6, ..Default::default() }
    }

    /// A synthetic scene with more range than any one exposure can hold, shot
    /// three times two stops apart, with the clipping a real sensor would do.
    fn bracket(shift: (i32, i32)) -> Vec<Frame> {
        let (w, h) = (128usize, 96usize);
        let scene = |x: usize, y: usize| {
            let mut radiance = 0.002 * 2f32.powf(10.0 * x as f32 / w as f32);
            for (bx, by, r) in [(20usize, 15usize, 9usize), (70, 60, 12), (100, 25, 7),
                                (40, 75, 10)] {
                let dx = x as f32 - bx as f32;
                let dy = y as f32 - by as f32;
                if dx * dx + dy * dy < (r * r) as f32 {
                    radiance *= 3.5;
                }
            }
            radiance
        };
        let mut frames = Vec::new();
        for (index, stops) in [-2.0f32, 0.0, 2.0].iter().enumerate() {
            let mut img = Image::new(w, h);
            let (dx, dy) = if index == 2 { shift } else { (0, 0) };
            for y in 0..h {
                for x in 0..w {
                    let sx = (x as i32 - dx).clamp(0, w as i32 - 1) as usize;
                    let sy = (y as i32 - dy).clamp(0, h as i32 - 1) as usize;
                    let seen = (scene(sx, sy) * 2f32.powf(*stops)).min(1.0); // the sensor clips
                    let i = (y * w + x) * 3;
                    for c in 0..3 {
                        img.d[i + c] = seen;
                    }
                }
            }
            frames.push(Frame {
                name: format!("{stops}"),
                linear: img,
                meta: meta(0.01 * 2f32.powf(*stops), 100.0),
                exif: Vec::new(),
            });
        }
        frames
    }

    /// The whole point: the merged frame has to hold range that no single
    /// exposure could, and hold it in the right proportions.
    #[test]
    fn the_merge_reaches_past_any_one_exposure() {
        let frames = bracket((0, 0));
        let single = &frames[1].linear;
        let (merged, notes) = merge(&frames, Options { align: false, deghost: 0.0 }).unwrap();

        assert_eq!(notes.reference, 1, "the middle exposure should be the reference");
        assert!((notes.range_stops - 4.0).abs() < 0.01, "bracket spans 4 stops: {notes:?}");

        // A band the middle exposure blew out completely.
        let row = (10 * 128) * 3;
        let flat: Vec<f32> = (118..126).map(|x| single.d[row + x * 3]).collect();
        let recovered: Vec<f32> = (118..126).map(|x| merged.d[row + x * 3]).collect();
        let spread = |v: &Vec<f32>| {
            v.iter().cloned().fold(f32::MIN, f32::max)
                / v.iter().cloned().fold(f32::MAX, f32::min).max(1e-9)
        };
        assert!(spread(&flat) < 1.02, "the single frame should be clipped flat: {flat:?}");
        assert!(spread(&recovered) > 1.3,
                "the merge should recover the highlight ramp: {recovered:?}");

        // Ten stops over 128 pixels is a doubling every 12.8, so two points a
        // sixteenth of the frame apart should differ by about 1.42x.
        let at = |x: usize| merged.d[row + x * 3];
        let ratio = at(120) / at(112);
        assert!(ratio > 1.25 && ratio < 1.65, "expected about 1.42x, got {ratio}");
    }

    /// A camera that moved between frames has to be put back.
    #[test]
    fn alignment_finds_a_shifted_frame() {
        let frames = bracket((3, -2));
        let anchor = median_bitmaps(&frames[1].linear);
        let moved = median_bitmaps(&frames[2].linear);
        // The merge reads frame j at (x + shift), so recovering a frame whose
        // content was moved by (3, -2) means asking for exactly that offset.
        let (dx, dy) = align(&anchor, &moved);
        assert_eq!((dx, dy), (3, -2), "should find the offset it was given");
    }

    /// Alignment has to be able to say "these are not the same picture".
    #[test]
    fn alignment_refuses_two_different_scenes() {
        let frames = bracket((0, 0));
        // Noise, which correlates with nothing, including itself at an offset.
        let mut other = Image::new(frames[1].linear.w, frames[1].linear.h);
        let mut state = 0x243F6A88u32;
        for v in other.d.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *v = state as f32 / u32::MAX as f32;
        }
        let shift = align(&median_bitmaps(&frames[1].linear), &median_bitmaps(&other));
        assert_eq!(shift, (0, 0), "nothing matched, so nothing should move");
    }

    /// Two frames is the minimum, and frames of different sizes are not a
    /// bracket at all.
    #[test]
    fn it_refuses_what_it_cannot_merge() {
        let frames = bracket((0, 0));
        assert!(merge(&frames[..1], Options::default()).is_err());
        let mut odd = bracket((0, 0));
        odd[2].linear = Image::new(10, 10);
        assert!(merge(&odd, Options::default()).is_err());
    }

    /// A reading sitting on the sensor's ceiling, or buried in its noise, must
    /// not be believed.
    #[test]
    fn clipped_and_buried_readings_are_not_trusted() {
        assert_eq!(reliability(0.999), 0.0, "a clipped pixel knows nothing");
        assert_eq!(reliability(0.0), 0.0, "neither does one below the noise");
        assert!(reliability(0.35) > 0.99, "a well exposed one is worth listening to");
    }
}

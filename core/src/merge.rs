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

/// Something about this bracket the photographer should be told.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub code: &'static str,
    pub frames: Vec<usize>,
    pub value: f32,
}

impl Finding {
    fn new(code: &'static str, frames: Vec<usize>, value: f32) -> Finding {
        Finding { code, frames, value }
    }
}

/// Everything wrong with a bracket that its metadata alone can reveal.
pub fn inspect(frames: &[Frame]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if frames.len() < 2 {
        return findings;
    }
    let base = &frames[0];
    let stops: Vec<f32> = frames.iter().map(|f| f.stops_from(base)).collect();
    let dials: Vec<f32> = frames.iter().map(|f| f.meta.exposure_comp).collect();

    // Frames that came out at the same exposure.
    let mut grouped = vec![false; frames.len()];
    for j in 0..frames.len() {
        if grouped[j] {
            continue;
        }
        let same: Vec<usize> = (0..frames.len())
            .filter(|k| (stops[*k] - stops[j]).abs() < SAME_EXPOSURE_EV)
            .collect();
        if same.len() < 2 {
            continue;
        }
        for k in &same {
            grouped[*k] = true;
        }
        // If they were dialled apart, the camera is what failed, and that has
        // a cause and a remedy worth naming separately.
        let dialled_apart = same.iter().any(|a| {
            same.iter().any(|b| (dials[*a] - dials[*b]).abs() > SAME_EXPOSURE_EV)
        });
        // How far apart they were asked to be, which is the range the
        // photographer thinks they have and does not.
        let apart = same.iter().flat_map(|a| same.iter().map(move |b| (a, b)))
            .map(|(a, b)| (dials[*a] - dials[*b]).abs())
            .fold(0.0f32, f32::max);
        let code = if dialled_apart { "dial_ignored" } else { "same_exposure" };
        findings.push(Finding::new(code, same, apart));
    }

    // A gap in the ladder. The tones that fall in it rest on one frame, so
    // they are as noisy, or as blown, as that one frame was.
    let mut ladder = stops.clone();
    ladder.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let gap = ladder.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
    if gap > WIDE_GAP_EV {
        findings.push(Finding::new("wide_gap", Vec::new(), gap));
    }

    // Frames that are not of the same setup.
    let odd = |pick: &dyn Fn(&Frame) -> String| -> Vec<usize> {
        let first = pick(&frames[0]);
        (0..frames.len()).filter(|j| pick(&frames[*j]) != first).collect()
    };
    for (code, differing) in [
        ("mixed_camera", odd(&|f: &Frame| format!("{} {}", f.meta.make, f.meta.model))),
        ("mixed_lens", odd(&|f: &Frame| f.meta.lens_model.clone())),
        // Rounded: a zoom reports a focal length to the nearest millimetre and
        // will not repeat it exactly between frames.
        ("mixed_focal", odd(&|f: &Frame| format!("{:.0}", f.meta.focal_mm))),
    ] {
        if !differing.is_empty() {
            findings.push(Finding::new(code, differing, 0.0));
        }
    }
    findings
}

#[derive(Clone, Debug, Default)]
pub struct Notes {
    pub reference: usize,
    /// Where each frame sits in the bracket, in stops from the reference, on
    /// the scale the merge actually used.
    pub stops: Vec<f32>,
    /// The exposure compensation the photographer dialled in, as the camera
    /// recorded it.
    pub bias: Vec<f32>,
    /// Where the pixels put each frame, for the frames that could be read.
    /// This is the one claim that cannot be wrong.
    pub measured: Vec<Option<f32>>,
    /// Frames whose metadata was contradicted by their own pixels, and which
    /// were therefore scaled by the measurement instead.
    pub remeasured: Vec<bool>,
    /// Everything the merge decided for itself, or found odd about the
    /// bracket, in the order it found it.
    pub findings: Vec<Finding>,
    /// The range the compensation dial was asking for, against `range_stops`,
    /// which is the range the frames actually cover.
    pub intended_range_stops: f32,
    pub shifts: Vec<(i32, i32)>,
    /// Fraction of the frame where the exposures disagreed enough to be
    /// treated as movement.
    pub ghosted: f32,
    /// Fraction of the frame no exposure captured well: blown in the darkest
    /// frame, or lost in the noise of the brightest.
    pub uncovered: f32,
    pub range_stops: f32,
}

// Reading the exposure off the pixels.
const MEASURE_EDGE: usize = 400;
const MEASURE_MIN_PIXELS: usize = 200;
// How far the metadata and the pixels have to disagree before the pixels win.
const METADATA_WRONG_EV: f32 = 0.75;
// And how tightly the frame has to agree with itself before that measurement
// is worth acting on.
const MEASURE_SPREAD_EV: f32 = 0.25;
// Two frames this close in exposure are the same exposure, whatever their
// compensation dials say about it.
const SAME_EXPOSURE_EV: f32 = 0.5;
// A step larger than this leaves the tones inside it resting on a single frame.
const WIDE_GAP_EV: f32 = 2.5;

/// Above this share of the compared pixels still disagreeing after the best
/// shift, the two frames are not showing the same thing.
const NO_CORRESPONDENCE: f32 = 0.22;

// A frame's reading is worth having when it is clear of the noise at the bottom
// and clear of the ceiling at the top.
const NOISE_FLOOR: f32 = 0.0015;
const TRUSTED_LOW: f32 = 0.02;
const TRUSTED_HIGH: f32 = 0.80;
const CEILING: f32 = 0.97;

/// How many stops apart two frames are according to their pixels, and how
/// tightly the frame agrees with itself about that number.
pub fn measured_stops(a: &Image, b: &Image) -> Option<(f32, f32)> {
    let (ta, tb) = (ops::thumbnail(a, MEASURE_EDGE), ops::thumbnail(b, MEASURE_EDGE));
    if ta.w != tb.w || ta.h != tb.h {
        return None;
    }
    let mut ratios = Vec::new();
    for i in 0..ta.w * ta.h {
        let la = ta.d[i * 3].max(ta.d[i * 3 + 1]).max(ta.d[i * 3 + 2]);
        let lb = tb.d[i * 3].max(tb.d[i * 3 + 1]).max(tb.d[i * 3 + 2]);
        // Both readings have to be off the noise floor and clear of the
        // ceiling, or the ratio measures a limit rather than the light.
        if la > TRUSTED_LOW && la < TRUSTED_HIGH && lb > TRUSTED_LOW && lb < TRUSTED_HIGH {
            ratios.push(lb / la);
        }
    }
    if ratios.len() < MEASURE_MIN_PIXELS {
        return None;
    }
    ratios.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let at = |f: f32| ratios[((ratios.len() - 1) as f32 * f) as usize];
    Some((at(0.5).log2(), (at(0.75) / at(0.25)).log2()))
}

/// Whether there is any signal here worth reading, from the brightest channel.
fn signal(level: f32) -> f32 {
    ops::smoothstep(NOISE_FLOOR, TRUSTED_LOW, level)
}

/// Whether this one channel still has room, so it is reporting the light rather
/// than its own ceiling.
fn headroom(value: f32) -> f32 {
    1.0 - ops::smoothstep(TRUSTED_HIGH, CEILING, value)
}

/// How much a reading of this level deserves to be believed, for a channel
/// that is the brightest in its pixel.
fn reliability(level: f32) -> f32 {
    signal(level) * headroom(level)
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
        bias: frames.iter().map(|f| f.meta.exposure_comp).collect(),
        measured: vec![None; frames.len()],
        remeasured: vec![false; frames.len()],
        shifts: vec![(0, 0); frames.len()],
        // Everything the metadata alone gives away.
        findings: inspect(frames),
        ..Notes::default()
    };

    // Check the metadata against the pixels before trusting it with the scale.
    for j in 0..frames.len() {
        if j == reference {
            continue;
        }
        let Some((measured, spread)) = measured_stops(&frames[reference].linear,
                                                      &frames[j].linear) else { continue };
        notes.measured[j] = Some(measured);
        if (measured - notes.stops[j]).abs() > METADATA_WRONG_EV && spread < MEASURE_SPREAD_EV {
            // Overruling a file's own metadata is the strongest thing this code
            // does to a photograph without being asked.
            notes.findings.push(Finding::new("metadata_overruled", vec![j],
                                             measured - notes.stops[j]));
            notes.stops[j] = measured;
            notes.remeasured[j] = true;
        }
    }

    let span = |v: &[f32]| v.iter().cloned().fold(f32::MIN, f32::max)
        - v.iter().cloned().fold(f32::MAX, f32::min);
    notes.range_stops = span(&notes.stops);
    notes.intended_range_stops = span(&notes.bias);

    if options.align {
        let anchor = median_bitmaps(&frames[reference].linear);
        let mut lost = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            if i != reference {
                match align(&anchor, &median_bitmaps(&frame.linear)) {
                    Some(shift) => notes.shifts[i] = shift,
                    // Not "no shift needed": the aligner could not find this
                    // frame in the reference at all.
                    None => lost.push(i),
                }
            }
        }
        if !lost.is_empty() {
            notes.findings.push(Finding::new("no_correspondence", lost, 0.0));
        }
    }

    // Everything is expressed in the reference frame's light, so a frame given
    // twice the exposure contributes half the value for the same radiance.
    let scale: Vec<f32> = notes.stops.iter().map(|s| (-s).exp2()).collect();

    // A frame given more light is a better measurement of the same radiance,
    // and not by a little.
    let brightest = scale.iter().cloned().fold(f32::MAX, f32::min);
    let snr: Vec<f32> = scale.iter().map(|s| brightest / s).collect();

    let n = w * h;
    let mut sum = vec![0f32; n * 3];
    // Per channel, because running out of range is per channel.
    let mut weight = vec![0f32; n * 3];
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
                let here = signal(level) * snr[j];
                if here > 0.0 {
                    for c in 0..3 {
                        let trust = here * headroom(px[c]);
                        if trust > 0.0 {
                            weight[dst + c] += trust;
                            sum[dst + c] += px[c] * scale[j] * trust;
                        }
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
        // A pixel can be short of one channel and not the others.
        let mut short = false;
        for c in 0..3 {
            if weight[i * 3 + c] > 1e-4 {
                out.d[i * 3 + c] = sum[i * 3 + c] / weight[i * 3 + c];
            } else {
                out.d[i * 3 + c] = fallback[i * 3 + c];
                short = true;
            }
        }
        if short {
            uncovered += 1;
            // All that is known about a channel with no usable reading is that
            // it was at least as bright as its ceiling, which is where the
            // fallback left it.
            let measured = (0..3)
                .filter(|c| weight[i * 3 + c] > 1e-4)
                .map(|c| out.d[i * 3 + c])
                .fold(0.0f32, f32::max);
            for c in 0..3 {
                if weight[i * 3 + c] <= 1e-4 {
                    out.d[i * 3 + c] = out.d[i * 3 + c].max(measured);
                }
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
fn align(anchor: &Bitmaps, other: &Bitmaps) -> Option<(i32, i32)> {
    let depth = anchor.levels.len().min(other.levels.len());
    if depth == 0 {
        return None;
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
                return None;
            }
        }
    }
    Some(shift)
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
        assert_eq!(align(&anchor, &moved), Some((3, -2)),
                   "should find the offset it was given");
    }

    /// Metadata is a claim, and a claim can be wrong.
    #[test]
    fn pixels_overrule_metadata_that_cannot_be_right() {
        let mut frames = bracket((0, 0));
        // The brightest frame now claims two stops more exposure than it was
        // actually given. Nothing about the picture changed, only the tag.
        let true_stops = frames[2].stops_from(&frames[1]);
        frames[2].meta.shutter_s *= 4.0;
        assert!((frames[2].stops_from(&frames[1]) - (true_stops + 2.0)).abs() < 1e-3);

        let (_, notes) = merge(&frames, Options { align: false, deghost: 0.0 }).expect("merge");
        assert!(notes.remeasured[2], "the lie should have been caught");
        assert!((notes.stops[2] - true_stops).abs() < 0.35,
                "should be put back where its pixels say it belongs: {} vs {true_stops}",
                notes.stops[2]);
        assert!(!notes.remeasured[0], "the honest frame should be left alone");
    }

    /// A bracket the camera could not deliver.
    #[test]
    fn a_bracket_the_camera_could_not_deliver_is_reported() {
        let mut frames = bracket((0, 0));
        // Same exposure as the middle frame, dialled three stops below it.
        frames[0].linear = frames[1].linear.clone();
        frames[0].meta = frames[1].meta.clone();
        frames[0].meta.exposure_comp = -3.0;
        frames[2].meta.exposure_comp = 2.0;

        let (_, notes) = merge(&frames, Options { align: false, deghost: 0.0 }).expect("merge");
        let told = notes.findings.iter().find(|f| f.code == "dial_ignored")
            .expect("the photographer should be told the camera did not deliver");
        assert_eq!(told.frames, vec![0, 1],
                   "and told which two frames came out the same");
        assert!(notes.intended_range_stops > notes.range_stops + 0.5,
                "less range arrived than was asked for: {} of {}",
                notes.range_stops, notes.intended_range_stops);
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
        assert_eq!(align(&median_bitmaps(&frames[1].linear), &median_bitmaps(&other)), None,
                   "nothing matched, so it should say so rather than report no shift");
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

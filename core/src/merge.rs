//! Combining a bracket of exposures into one frame that holds all of them.

use crate::decode::{self, Meta};
use kit::Image;

/// One exposure of the bracket, decoded but otherwise untouched.
pub struct Frame {
    pub name: String,
    pub linear: Image,
    pub meta: Meta,
    /// The tags this frame was shot with.
    pub exif: Vec<crate::exif::Entry>,
}

impl Frame {
    pub fn open(name: &str, bytes: &[u8]) -> Result<Frame, decode::DecodeError> {
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
    pub deghost: Option<f32>,
}

impl Default for Options {
    fn default() -> Self {
        Options { align: true, deghost: None }
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

/// Whether two frames are the same camera, through the same glass, at the same
/// focal length.
fn same_setup(a: &Frame, b: &Frame) -> bool {
    let (x, y) = (&a.meta, &b.meta);
    x.make == y.make
        && x.model == y.model
        && x.lens_model == y.lens_model
        // Rounded, as the mixed_focal check rounds.
        && format!("{:.0}", x.focal_mm) == format!("{:.0}", y.focal_mm)
}

/// How long the camera was left alone between two frames, or `None` when one of
/// them does not say when it was taken.
fn pause_between(a: &Frame, b: &Frame) -> Option<i64> {
    if a.meta.captured_s == 0 || b.meta.captured_s == 0 {
        return None;
    }
    // The exposure comes off the gap rather than the gap being compared raw.
    let exposure = a.meta.shutter_s.max(b.meta.shutter_s).max(0.0).ceil() as i64;
    Some(((b.meta.captured_s - a.meta.captured_s).abs() - exposure).max(0))
}

/// Whether one frame is the next frame of the same bracket as the other.
fn one_burst(a: &Frame, b: &Frame) -> bool {
    match pause_between(a, b) {
        Some(pause) => pause <= BURST_GAP_S,
        // A frame with no capture time cannot be placed on the clock at all.
        None => true,
    }
}

/// Which of the frames handed in actually belong to the same bracket.
pub fn group(frames: &[Frame]) -> Vec<Vec<usize>> {
    // Sorted, because "the same burst" is a claim about neighbours in time and
    // the interface hands frames over in whatever order the file picker gave
    // it.
    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by_key(|&j| (frames[j].meta.captured_s == 0, frames[j].meta.captured_s, j));

    let mut groups: Vec<Vec<usize>> = Vec::new();
    for j in order {
        let joins = groups.last().is_some_and(|g| {
            let previous = &frames[*g.last().unwrap()];
            same_setup(previous, &frames[j]) && one_burst(previous, &frames[j])
        });
        if joins {
            groups.last_mut().unwrap().push(j);
        } else {
            groups.push(vec![j]);
        }
    }
    groups
}

/// Everything wrong with a bracket that its metadata alone can reveal.
pub fn inspect(frames: &[Frame]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let groups = group(frames);

    // The split comes first.
    if groups.len() > 1 {
        for (nth, members) in groups.iter().enumerate() {
            findings.push(Finding::new("separate_shot", members.clone(), nth as f32 + 1.0));
        }
        // And where a split was a judgement rather than a measurement, say
        // which seam and how long the pause was, so it can be overruled by
        // someone who was there.
        for pair in groups.windows(2) {
            let last = *pair[0].last().unwrap();
            let first = pair[1][0];
            let Some(pause) = pause_between(&frames[last], &frames[first]) else { continue };
            if pause < UNRELATED_GAP_S && same_setup(&frames[last], &frames[first]) {
                findings.push(Finding::new("borderline_split", vec![last, first], pause as f32));
            }
        }
    }

    // Frames the clock could not place at all.
    let undated: Vec<usize> =
        (0..frames.len()).filter(|j| frames[*j].meta.captured_s == 0).collect();
    if !undated.is_empty() && frames.len() > 1 {
        findings.push(Finding::new("undated", undated, 0.0));
    }

    // The exposure ladder is a question about one bracket, so it is asked once
    // per bracket.
    for members in &groups {
        findings.extend(inspect_bracket(frames, members));
    }

    // There used to be a mixed_camera / mixed_lens / mixed_focal check here,
    // asked of the whole pile.
    findings
}

/// The same question asked of a single bracket.
fn inspect_bracket(frames: &[Frame], members: &[usize]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if members.len() < 2 {
        return findings;
    }
    let n = members.len();
    let at = |k: usize| &frames[members[k]];
    let base = at(0);
    let stops: Vec<f32> = (0..n).map(|k| at(k).stops_from(base)).collect();
    let dials: Vec<f32> = (0..n).map(|k| at(k).meta.exposure_comp).collect();

    // Frames that came out at the same exposure.
    let mut grouped = vec![false; n];
    for j in 0..n {
        if grouped[j] {
            continue;
        }
        let same: Vec<usize> = (0..n)
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
        findings.push(Finding::new(code, same.iter().map(|k| members[*k]).collect(), apart));
    }

    // A gap in the ladder. The tones that fall in it rest on one frame, so
    // they are as noisy, or as blown, as that one frame was.
    let mut ladder = stops.clone();
    ladder.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let gap = ladder.windows(2).map(|w| w[1] - w[0]).fold(0.0f32, f32::max);
    if gap > WIDE_GAP_EV {
        findings.push(Finding::new("wide_gap", Vec::new(), gap));
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
    /// The ghost removal that was used, measured or as given.
    pub deghost_used: f32,
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
// How long a pause can be and still be inside one bracket, before the exposures
// themselves are added on.
const BURST_GAP_S: i64 = 15;
// Past this pause a split is no longer a judgement call, so there is nothing to
// ask about.
const UNRELATED_GAP_S: i64 = 126;
// Two frames this close in exposure are the same exposure, whatever their
// compensation dials say about it.
const SAME_EXPOSURE_EV: f32 = 0.5;

// Deghosting.
const GHOST_LOOSE_SIGMAS: f32 = 16.0;
const GHOST_TIGHT_SIGMAS: f32 = 6.0;
// What the measured setting uses, and what the probe that measures it uses:
// comfortably past anything noise reaches, so what it counts really moved.
const GHOST_PROBE_SIGMAS: f32 = 10.0;
// No threshold goes below this, whatever the arithmetic says: see
// `ghost_tolerance`.
const GHOST_FLOOR_EV: f32 = 0.25;
// Less of the frame than this disagreeing is not a moving subject, it is the
// last of the noise, and substituting for it costs more than it saves.
const GHOST_QUIET: f32 = 0.0002;
// More of the frame than this is not ghosting either.
const GHOST_WHOLESALE: f32 = 0.25;
// The centre crop the noise constant is read from, matching what the developer
// does for the same measurement.
const PROBE_CROP: usize = 640;
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
    let (ta, tb) = (kit::thumbnail(a, MEASURE_EDGE), kit::thumbnail(b, MEASURE_EDGE));
    if ta.w != tb.w || ta.h != tb.h {
        return None;
    }
    let mut ratios = Vec::new();
    for (pa, pb) in ta.px().iter().zip(tb.px()) {
        let la = pa[0].max(pa[1]).max(pa[2]);
        let lb = pb[0].max(pb[1]).max(pb[2]);
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

/// The spread shot noise alone puts on a reading, in stops.
pub fn noise_stops(level: f32, k: f32) -> f32 {
    k / (level.max(NOISE_FLOOR).sqrt() * std::f32::consts::LN_2)
}

/// The shot-noise constant of a frame, read off the frame itself.
const FLOOR_TO_SIGMA: f32 = 5.3;

pub fn shot_noise_constant(img: &Image) -> f32 {
    (2.0 * FLOOR_TO_SIGMA * crate::analyze::centre_noise_floor(img, PROBE_CROP)).max(1e-6)
}

/// How far two readings may differ before the difference is worth calling
/// movement, in stops.
fn ghost_tolerance(level: f32, k: f32, sigmas: f32) -> f32 {
    (sigmas * noise_stops(level, k)).max(GHOST_FLOOR_EV)
}

/// Whether there is any signal here worth reading, from the brightest channel.
fn signal(level: f32) -> f32 {
    kit::smoothstep(NOISE_FLOOR, TRUSTED_LOW, level)
}

/// Whether this one channel still has room, so it is reporting the light rather
/// than its own ceiling.
fn headroom(value: f32) -> f32 {
    1.0 - kit::smoothstep(TRUSTED_HIGH, CEILING, value)
}

/// Whether the reference frame is entitled to an opinion about this pixel.
pub fn arbitrable(level: f32) -> bool {
    signal(level) > 0.0 && level < TRUSTED_HIGH
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
        let src_px = frame.linear.px();
        for y in overlap(h, dy) {
            let sy = (y as i32 + dy) as usize;
            for x in overlap(w, dx) {
                let sx = (x as i32 + dx) as usize;
                let dst = (y * w + x) * 3;
                let px = src_px[sy * w + sx];
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

    // What the sensor's own noise looks like in this frame, which is what the
    // deghosting threshold is denominated in.
    let k = shot_noise_constant(&frames[reference].linear);
    let sigmas = match options.deghost {
        // The control, mapped onto the thing it was always trying to say.
        Some(amount) if amount <= 0.0 => None,
        Some(amount) => {
            let a = amount.clamp(0.0, 1.0);
            Some(GHOST_LOOSE_SIGMAS + a * (GHOST_TIGHT_SIGMAS - GHOST_LOOSE_SIGMAS))
        }
        // Or measure it. Count what disagrees by more than noise can account
        // for, and let that decide whether there is anything to remove.
        None => {
            let moving = probe_movement(&out, frames, &scale, &notes.shifts, reference, k);
            // Carried as a percentage, because that is the unit the sentence
            // written about it reads in, the way the other findings carry
            // stops.
            let found = |code| Finding::new(code, Vec::new(), moving * 100.0);
            if moving < GHOST_QUIET {
                notes.findings.push(found("movement_none"));
                None
            } else if moving > GHOST_WHOLESALE {
                notes.findings.push(found("movement_wholesale"));
                None
            } else {
                notes.findings.push(found("movement"));
                Some(GHOST_PROBE_SIGMAS)
            }
        }
    };
    if let Some(sigmas) = sigmas {
        notes.deghost_used = ((GHOST_LOOSE_SIGMAS - sigmas)
            / (GHOST_LOOSE_SIGMAS - GHOST_TIGHT_SIGMAS)).clamp(0.0, 1.0);
        notes.ghosted = deghost(&mut out, frames, &scale, &notes.shifts, reference, k, sigmas);
    }
    Ok((out, notes))
}

/// Walk the pixels the reference frame is entitled to arbitrate.
fn visit(frames: &[Frame], shifts: &[(i32, i32)], reference: usize, scale: &[f32],
         w: usize, h: usize, mut look: impl FnMut(usize, f32, [f32; 3])) {
    let (dx, dy) = shifts[reference];
    let s = scale[reference];
    for y in 0..h {
        let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
        for x in 0..w {
            let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
            let src = (sy * w + sx) * 3;
            let px = [frames[reference].linear.d[src], frames[reference].linear.d[src + 1],
                      frames[reference].linear.d[src + 2]];
            let level = px[0].max(px[1]).max(px[2]);
            if !arbitrable(level) {
                continue;
            }
            look((y * w + x) * 3, level, [px[0] * s, px[1] * s, px[2] * s]);
        }
    }
}

/// How far apart two readings of the same pixel are, in stops.
fn disagreement(mine: [f32; 3], theirs: [f32; 3]) -> f32 {
    let a = mine[0].max(mine[1]).max(mine[2]).max(1e-6);
    let b = theirs[0].max(theirs[1]).max(theirs[2]).max(1e-6);
    (a / b).log2().abs()
}

/// What share of the arbitrable frame disagrees by more than noise explains,
/// without changing anything.
fn probe_movement(out: &Image, frames: &[Frame], scale: &[f32], shifts: &[(i32, i32)],
                  reference: usize, k: f32) -> f32 {
    let mut moved = 0usize;
    let mut looked = 0usize;
    visit(frames, shifts, reference, scale, out.w, out.h, |dst, level, mine| {
        looked += 1;
        let theirs = [out.d[dst], out.d[dst + 1], out.d[dst + 2]];
        if disagreement(mine, theirs) > ghost_tolerance(level, k, GHOST_PROBE_SIGMAS) {
            moved += 1;
        }
    });
    if looked == 0 { 0.0 } else { moved as f32 / looked as f32 }
}

/// Replace pixels where the frames disagree about the scene with the reference
/// frame's own reading.
fn deghost(out: &mut Image, frames: &[Frame], scale: &[f32], shifts: &[(i32, i32)],
           reference: usize, k: f32, sigmas: f32) -> f32 {
    let (w, h) = (out.w, out.h);
    let mut moved = 0usize;
    visit(frames, shifts, reference, scale, w, h, |dst, level, mine| {
        let theirs = [out.d[dst], out.d[dst + 1], out.d[dst + 2]];
        if disagreement(mine, theirs) > ghost_tolerance(level, k, sigmas) {
            moved += 1;
            for c in 0..3 {
                out.d[dst + c] = mine[c];
            }
        }
    });
    moved as f32 / (w * h) as f32
}


// -------------------------------------------------------------------------
// alignment
// -------------------------------------------------------------------------

/// A frame reduced to what survives a change of exposure.
pub struct Bitmaps {
    levels: Vec<(Vec<bool>, Vec<bool>, usize, usize)>, // above median, worth counting, w, h
}

/// Threshold a frame at its own median.
pub fn median_bitmaps(img: &Image) -> Bitmaps {
    // Work in a perceptual-ish scale so the median sits somewhere useful.
    let grey = crate::analyze::sqrt_luminance(img);
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
        plane = kit::resize_plane(&plane, plane.w / 2, plane.h / 2);
    }
    Bitmaps { levels }
}

/// The shift that best lines up two frames, found coarse to fine.
pub fn align(anchor: &Bitmaps, other: &Bitmaps) -> Option<(i32, i32)> {
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

/// How many of the pixels worth comparing disagree, and how many were worth
/// comparing.
fn mismatch(a: &[bool], a_use: &[bool], b: &[bool], b_use: &[bool], w: usize, h: usize,
            (dx, dy): (i32, i32)) -> (usize, usize) {
    let mut wrong = 0usize;
    let mut compared = 0usize;
    for y in overlap(h, dy) {
        let sy = (y as i32 + dy) as usize;
        for x in overlap(w, dx) {
            let i = y * w + x;
            let j = sy * w + (x as i32 + dx) as usize;
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

/// The positions along an axis of `len` whose neighbour `d` away is also on it.
fn overlap(len: usize, d: i32) -> std::ops::Range<usize> {
    let len = len as i32;
    (-d).clamp(0, len) as usize..(len - d).clamp(0, len) as usize
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
        let (merged, notes) = merge(&frames, Options { align: false, deghost: Some(0.0) }).unwrap();

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

        let (_, notes) = merge(&frames, Options { align: false, deghost: Some(0.0) }).expect("merge");
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

        let (_, notes) = merge(&frames, Options { align: false, deghost: Some(0.0) }).expect("merge");
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

    /// A handheld bracket turns a little as well as shifting, and the aligner
    /// answers with a translation only.
    #[test]
    fn alignment_absorbs_a_handheld_rotation() {
        let (w, h) = (128usize, 96usize);
        let scene = |x: f32, y: f32| {
            let mut radiance = 0.002 * 2f32.powf(10.0 * x / w as f32);
            // Texture, because a bare ramp is a trap for a median threshold.
            radiance *= 1.0 + 0.6 * (x * 0.7).sin() * (y * 0.9).sin();
            for (bx, by, r) in [(20.0f32, 15.0f32, 9.0f32), (70.0, 60.0, 12.0),
                                (100.0, 25.0, 7.0), (40.0, 75.0, 10.0)] {
                let (dx, dy) = (x - bx, y - by);
                if dx * dx + dy * dy < r * r {
                    radiance *= 3.5;
                }
            }
            radiance
        };
        let paint = |stops: f32, shift: (i32, i32), degrees: f32| {
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let (sin, cos) = degrees.to_radians().sin_cos();
            let mut img = Image::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    // Undo the shift and then the turn, so the content ends up
                    // moved by both, the way the merge reads a frame back.
                    let ux = x as f32 - shift.0 as f32 - cx;
                    let uy = y as f32 - shift.1 as f32 - cy;
                    let sx = (cos * ux + sin * uy + cx).clamp(0.0, w as f32 - 1.0);
                    let sy = (-sin * ux + cos * uy + cy).clamp(0.0, h as f32 - 1.0);
                    let seen = (scene(sx, sy) * 2f32.powf(stops)).min(1.0);
                    let i = (y * w + x) * 3;
                    for c in 0..3 {
                        img.d[i + c] = seen;
                    }
                }
            }
            img
        };

        let anchor = median_bitmaps(&paint(0.0, (0, 0), 0.0));
        let turned = median_bitmaps(&paint(3.0, (3, -2), 1.5));
        assert_eq!(align(&anchor, &turned), Some((3, -2)),
                   "a turn this size should leave the translation standing");
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
        assert_eq!(headroom(0.999), 0.0, "a clipped pixel knows nothing");
        assert_eq!(signal(0.0), 0.0, "neither does one below the noise");
        assert!(signal(0.35) * headroom(0.35) > 0.99,
                "a well exposed one is worth listening to");
        // And the pair of them are what decides whether the reference frame
        // gets a say, which is where believing a clipped reading did real harm.
        assert!(!arbitrable(0.999), "a clipped reference frame arbitrated anyway");
        assert!(!arbitrable(0.0), "a reference frame below its noise arbitrated anyway");
        assert!(arbitrable(0.35), "a well exposed reference frame was not allowed to speak");
    }

    /// One frame, described the way the grouping reads it: when it was taken,
    /// how long for, and through what.
    fn shot(name: &str, captured_s: i64, shutter: f32, lens: &str, focal: f32) -> Frame {
        Frame {
            name: name.to_string(),
            linear: Image::new(1, 1),
            meta: Meta {
                make: "SONY".into(),
                model: "ILCE-5000".into(),
                lens_model: lens.into(),
                focal_mm: focal,
                captured_s,
                ..meta(shutter, 100.0)
            },
            exif: Vec::new(),
        }
    }

    /// The real bracket on disk: three frames at 09:07:40, :41 and :41,
    /// through one lens at 35 mm. One answer, not three.
    #[test]
    fn a_burst_is_one_bracket() {
        let frames = vec![
            shot("DSC05718", 1_790_068_060, 1.0 / 4000.0, "E 35mm F1.8", 35.0),
            shot("DSC05719", 1_790_068_061, 1.0 / 4000.0, "E 35mm F1.8", 35.0),
            shot("DSC05720", 1_790_068_061, 1.0 / 500.0, "E 35mm F1.8", 35.0),
        ];
        assert_eq!(group(&frames), vec![vec![0, 1, 2]]);
        // And nothing in the findings claims it is more than one shot.
        assert!(!inspect(&frames).iter().any(|f| f.code == "separate_shot"));
    }

    /// The pair that shares every tag but the clock.
    #[test]
    fn photographs_minutes_apart_are_not_a_bracket() {
        let frames = vec![
            shot("DSC05549", 1_789_242_651, 1.0 / 1250.0, "E PZ 16-50mm F3.5-5.6 OSS", 50.0),
            shot("DSC05563", 1_789_243_156, 1.0 / 1600.0, "E PZ 16-50mm F3.5-5.6 OSS", 50.0),
        ];
        assert_eq!(group(&frames), vec![vec![0], vec![1]],
                   "two photographs, each its own answer");
        let findings = inspect(&frames);
        assert_eq!(findings.iter().filter(|f| f.code == "separate_shot").count(), 2);
    }

    /// The case the whole threshold was set for: two genuine brackets, a minute
    /// apart, everything else identical. Two answers.
    #[test]
    fn two_brackets_a_minute_apart_come_back_as_two() {
        let mut frames = Vec::new();
        for (n, base) in [0i64, 60].iter().enumerate() {
            for step in 0..3i64 {
                frames.push(shot(&format!("bracket {n} frame {step}"),
                                 1_790_000_000 + base + step,
                                 1.0 / 4000.0 * 4f32.powi(step as i32),
                                 "E 35mm F1.8", 35.0));
            }
        }
        assert_eq!(group(&frames), vec![vec![0, 1, 2], vec![3, 4, 5]]);
        // A minute is well inside the band where the split is a judgement, so
        // it has to be offered as one rather than performed quietly.
        let findings = inspect(&frames);
        let borderline = findings.iter().find(|f| f.code == "borderline_split")
            .expect("a minute apart is close enough to be worth querying");
        assert_eq!(borderline.frames, vec![2, 3]);
        assert!((borderline.value - 58.0).abs() < 1.5,
                "the pause it reports is the one between the brackets, got {}",
                borderline.value);
    }

    /// A bracket of long exposures must not be split by its own shutter speed.
    #[test]
    fn a_long_exposure_bracket_survives_its_own_shutter() {
        let frames = vec![
            shot("short", 1_790_000_000, 8.0, "E 35mm F1.8", 35.0),
            shot("long", 1_790_000_032, 30.0, "E 35mm F1.8", 35.0),
        ];
        assert_eq!(group(&frames), vec![vec![0, 1]],
                   "32 seconds apart, but 30 of them were the exposure");
    }

    /// Simultaneous frames through different glass are not one bracket, however
    /// well the clock agrees. Two bodies shooting one event is the case.
    #[test]
    fn the_same_instant_through_a_different_lens_is_a_different_shot() {
        let frames = vec![
            shot("a", 1_790_000_000, 1.0 / 250.0, "E 35mm F1.8", 35.0),
            shot("b", 1_790_000_000, 1.0 / 250.0, "E PZ 16-50mm F3.5-5.6 OSS", 35.0),
            shot("c", 1_790_000_000, 1.0 / 250.0, "E 35mm F1.8", 16.0),
        ];
        assert_eq!(group(&frames).len(), 3, "different lens, and different focal length");
    }

    /// A frame with no capture time cannot be placed on the clock.
    #[test]
    fn undated_frames_are_admitted_to_rather_than_guessed_at() {
        let frames = vec![
            shot("no date", 0, 1.0 / 250.0, "E 35mm F1.8", 35.0),
            shot("no date either", 0, 1.0 / 60.0, "E 35mm F1.8", 35.0),
        ];
        assert_eq!(group(&frames), vec![vec![0, 1]],
                   "nothing says they are apart, so they stay as handed in");
        let findings = inspect(&frames);
        let told = findings.iter().find(|f| f.code == "undated").expect("said out loud");
        assert_eq!(told.frames, vec![0, 1]);
    }

    /// Frames arrive in whatever order the file picker produced them, and the
    /// grouping is a claim about neighbours in time, so it sorts first.
    #[test]
    fn the_order_they_arrive_in_does_not_matter() {
        let shuffled = vec![
            shot("late bracket b", 1_790_009_001, 1.0 / 250.0, "E 35mm F1.8", 35.0),
            shot("early single", 1_790_000_000, 1.0 / 250.0, "E 35mm F1.8", 35.0),
            shot("late bracket a", 1_790_009_000, 1.0 / 60.0, "E 35mm F1.8", 35.0),
        ];
        assert_eq!(group(&shuffled), vec![vec![1], vec![2, 0]]);
    }

    /// A pile of unrelated photographs is not a bracket with problems, it is
    /// not a bracket. Every frame comes back as itself.
    #[test]
    fn a_pile_of_unrelated_photographs_refuses_to_be_a_stack() {
        let frames: Vec<Frame> = (0..5)
            .map(|j| shot(&format!("frame {j}"), 1_790_000_000 + j * 3_600,
                          1.0 / 250.0, "E 35mm F1.8", 35.0))
            .collect();
        let groups = group(&frames);
        assert_eq!(groups.len(), 5, "an hour apart each, so five photographs");
        assert!(groups.iter().all(|g| g.len() == 1));
        // An hour is past any judgement call, so there is nothing to query.
        assert!(!inspect(&frames).iter().any(|f| f.code == "borderline_split"));
    }
    /// Shot noise, laid over a bracket so the deghosting has something to be
    /// confused by.
    fn add_shot_noise(frames: &mut [Frame], strength: f32) {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut unit = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 40) as f32 / 16_777_216.0
        };
        for frame in frames.iter_mut() {
            for v in frame.linear.d.iter_mut() {
                // Four uniforms make a passable normal, once the variance of
                // their sum is scaled back to one.
                let z = (unit() + unit() + unit() + unit() - 2.0) * 1.732;
                *v = (*v + strength * v.max(0.0).sqrt() * z).clamp(0.0, 1.0);
            }
        }
    }

    /// A rectangle of one frame filled with what lies to its left, which is
    /// what an object moving between exposures does to a bracket.
    const MOVED: (usize, usize, usize, usize) = (44, 20, 40, 56);
    const MOVED_BY: usize = 24;

    fn displace(frame: &mut Frame) {
        let (x0, y0, pw, ph) = MOVED;
        let w = frame.linear.w;
        let source = frame.linear.d.clone();
        for y in y0..y0 + ph {
            for x in x0..x0 + pw {
                let from = (y * w + x - MOVED_BY) * 3;
                let to = (y * w + x) * 3;
                for c in 0..3 {
                    frame.linear.d[to + c] = source[from + c];
                }
            }
        }
    }

    fn moved_patch_contains(x: usize, y: usize) -> bool {
        let (x0, y0, pw, ph) = MOVED;
        x >= x0 && x < x0 + pw && y >= y0 && y < y0 + ph
    }

    /// Which pixels a setting substituted, found by merging twice and
    /// comparing, so the test cannot drift away from the rule it is checking.
    fn substituted(frames: &[Frame], deghost: Option<f32>) -> (usize, usize, usize) {
        let off = Options { align: false, deghost: Some(0.0) };
        let (clean, notes) = merge(frames, off).unwrap();
        let (out, _) = merge(frames, Options { align: false, deghost }).unwrap();
        let reference = &frames[notes.reference].linear;
        let (w, h) = (clean.w, clean.h);
        let (mut inside, mut outside, mut arbitrable_in_patch) = (0, 0, 0);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 3;
                let level = reference.d[i].max(reference.d[i + 1]).max(reference.d[i + 2]);
                if !arbitrable(level) {
                    continue;
                }
                if moved_patch_contains(x, y) {
                    arbitrable_in_patch += 1;
                }
                if (0..3).any(|c| out.d[i + c] != clean.d[i + c]) {
                    if moved_patch_contains(x, y) {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
        }
        (inside, outside, arbitrable_in_patch)
    }

    /// What the probe measured, back out of the percentage the finding
    /// reports it in.
    fn measured(notes: &Notes) -> f32 {
        notes.findings.iter().find(|f| f.code.starts_with("movement")).unwrap().value / 100.0
    }

    /// The bug this whole change exists for.
    #[test]
    fn noise_alone_is_not_movement() {
        let mut frames = bracket((0, 0));
        add_shot_noise(&mut frames, 0.002);
        let (_, notes) = merge(&frames, Options { align: false, deghost: None }).unwrap();
        let moving = measured(&notes);
        assert!(moving < GHOST_QUIET, "nothing moved, yet {moving} of the frame was called movement");
        assert!(moving < GHOST_WHOLESALE, "the wholesale guard, not the noise model, would be deciding this");
        assert!(notes.findings.iter().any(|f| f.code == "movement_none"),
                "the merge decided to remove nothing without saying so");
        assert_eq!(notes.ghosted, 0.0, "the merge was substituted into on a static scene");
        assert_eq!(notes.deghost_used, 0.0);
    }

    /// And the other half: something that did move still gets caught.
    #[test]
    fn real_movement_is_still_caught() {
        let mut frames = bracket((0, 0));
        add_shot_noise(&mut frames, 0.002);
        // The longest exposure carries the most weight down here, so moving
        // part of it is the case the merge is least able to ignore.
        displace(&mut frames[2]);
        let (_, notes) = merge(&frames, Options { align: false, deghost: None }).unwrap();
        let moving = measured(&notes);
        assert!(moving > GHOST_QUIET, "a displaced patch went unnoticed: {moving}");
        assert!(notes.findings.iter().any(|f| f.code == "movement"),
                "movement was acted on without saying so");
        assert!(notes.deghost_used > 0.0, "movement was found and then not acted on");

        let (inside, outside, patch) = substituted(&frames, None);
        let caught = inside as f32 / patch as f32;
        let elsewhere = outside as f32 / (patch * 4).max(1) as f32;
        assert!(caught > 0.8, "only {caught} of the moving patch was rejected");
        assert!(elsewhere < 0.05, "{elsewhere} of the still part of the scene was rejected too");
    }

    /// The control has to keep working, and keep pointing the way it reads.
    #[test]
    fn the_control_overrides_the_measurement() {
        let mut frames = bracket((0, 0));
        add_shot_noise(&mut frames, 0.002);
        displace(&mut frames[2]);
        let at = |amount: f32| {
            let options = Options { align: false, deghost: Some(amount) };
            merge(&frames, options).unwrap().1
        };
        assert_eq!(at(0.0).ghosted, 0.0, "a control reading none still removed something");
        assert!(at(1.0).ghosted >= at(0.5).ghosted, "the control is not monotonic");
        assert_eq!(at(0.5).deghost_used, 0.5, "an explicit setting was not the one used");
        assert!(at(0.5).findings.iter().all(|f| !f.code.starts_with("movement")),
                "an overridden merge reported a measurement it did not make");
    }

}

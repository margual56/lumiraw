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
    /// How near each pixel came to clipping on the sensor, in 255ths
    /// (`Decoded::saturation`); empty when not known, and then the frame's
    /// own values are all there is to judge by.
    pub saturation: Vec<u8>,
}

impl Frame {
    pub fn open(name: &str, bytes: &[u8]) -> Result<Frame, decode::DecodeError> {
        let decoded = decode::decode_with_saturation(bytes)?;
        Ok(Frame { name: name.to_string(), linear: decoded.img, meta: decoded.meta,
                   exif: decode::collect_exif(bytes), saturation: decoded.saturation })
    }

    /// How full the sensor was at a pixel, 0..1: its fullest channel against
    /// that channel's ceiling. Without the sensor's word for it, the brightest
    /// value the pixel has.
    pub fn fullness(&self, i: usize, px: [f32; 3]) -> f32 {
        match self.saturation.get(i) {
            Some(v) => *v as f32 / 255.0,
            None => px[0].max(px[1]).max(px[2]),
        }
    }

    /// Whether this frame's values at pixel `i` are clear of the sensor's
    /// ceiling, 0..1, per output channel.
    fn headroom_at(&self, i: usize, px: [f32; 3]) -> [f32; 3] {
        match self.saturation.get(i) {
            // One channel clipped on the sensor spoils all three once the
            // colour matrix has mixed them, and highlight reconstruction has
            // filled it with a guess.
            Some(v) => [headroom(*v as f32 / 255.0); 3],
            None => [headroom(px[0]), headroom(px[1]), headroom(px[2])],
        }
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
    /// Where each frame was read for each pixel of the merged frame; `shifts`
    /// is what these come to at its middle.
    pub warps: Vec<Warp>,
    /// Fraction of the frame where the exposures disagreed enough to be
    /// treated as movement.
    pub ghosted: f32,
    /// Fraction of the frame no exposure captured well: blown in the darkest
    /// frame, or lost in the noise of the brightest.
    pub uncovered: f32,
    pub range_stops: f32,
    /// How many stops below the reference frame's light the merged frame is
    /// expressed in: it is in the darkest frame's, so a value at 1 was clipped
    /// in every frame.
    pub units_stops: f32,
}

// Reading the exposure off the pixels.
pub(crate) const MEASURE_EDGE: usize = 400;
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
pub(crate) const TRUSTED_LOW: f32 = 0.02;
const TRUSTED_HIGH: f32 = 0.80;
const CEILING: f32 = 0.97;

/// How many stops apart two frames are according to their pixels, and how
/// tightly the frame agrees with itself about that number.
pub fn measured_stops(a: &Image, b: &Image) -> Option<(f32, f32)> {
    measured_stops_clear_of(a, &[], b, &[])
}

/// `measured_stops`, leaving out what either frame's sensor clipped (see
/// `Frame::saturation`; empty when not known).
pub fn measured_stops_clear_of(a: &Image, a_full: &[u8], b: &Image, b_full: &[u8])
                               -> Option<(f32, f32)> {
    let (ta, tb) = (kit::thumbnail(a, MEASURE_EDGE), kit::thumbnail(b, MEASURE_EDGE));
    if ta.w != tb.w || ta.h != tb.h {
        return None;
    }
    let (fa, fb) = (fullest(a_full, a, &ta), fullest(b_full, b, &tb));
    let mut ratios = Vec::new();
    for (i, (pa, pb)) in ta.px().iter().zip(tb.px()).enumerate() {
        let la = pa[0].max(pa[1]).max(pa[2]);
        let lb = pb[0].max(pb[1]).max(pb[2]);
        let clear = |full: &[f32], level: f32| full.get(i).copied().unwrap_or(level) < TRUSTED_HIGH;
        // Both readings have to be off the noise floor and clear of the
        // ceiling, or the ratio measures a limit rather than the light.
        if la > TRUSTED_LOW && clear(&fa, la) && lb > TRUSTED_LOW && clear(&fb, lb) {
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

/// Whether a frame's pixels put it far enough from where its metadata says,
/// and agree with themselves closely enough, to be believed over it.
pub fn overrules(claimed: f32, measured: f32, spread: f32) -> bool {
    (measured - claimed).abs() > METADATA_WRONG_EV && spread < MEASURE_SPREAD_EV
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
    arbitrable_at(level, level)
}

/// `arbitrable`, with how full the sensor was there (`Frame::fullness`).
fn arbitrable_at(level: f32, fullness: f32) -> bool {
    signal(level) > 0.0 && fullness < TRUSTED_HIGH
}

/// The fullest any sensor pixel got under each pixel of `thumb`, a smaller
/// copy of `full_size`; empty when `saturation` is.
fn fullest(saturation: &[u8], full_size: &Image, thumb: &Image) -> Vec<f32> {
    if saturation.len() != full_size.w * full_size.h {
        return Vec::new();
    }
    let mut out = vec![0f32; thumb.w * thumb.h];
    for y in 0..full_size.h {
        let ty = (y * thumb.h / full_size.h).min(thumb.h - 1);
        for x in 0..full_size.w {
            let tx = (x * thumb.w / full_size.w).min(thumb.w - 1);
            let v = saturation[y * full_size.w + x] as f32 / 255.0;
            let o = &mut out[ty * thumb.w + tx];
            *o = o.max(v);
        }
    }
    out
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
        let (a, b) = (&frames[reference], &frames[j]);
        let Some((measured, spread)) = measured_stops_clear_of(&a.linear, &a.saturation,
                                                               &b.linear, &b.saturation)
            else { continue };
        notes.measured[j] = Some(measured);
        if overrules(notes.stops[j], measured, spread) {
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

    // Everything is expressed in the reference frame's light, so a frame given
    // twice the exposure contributes half the value for the same radiance.
    let scale: Vec<f32> = notes.stops.iter().map(|s| (-s).exp2()).collect();

    let mut warps = vec![Warp::IDENTITY; frames.len()];
    if options.align {
        let mut lost = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            if i != reference {
                // Both frames as the light both of them recorded, so that
                // the difference between them is where things are.
                let (lo, hi) = shared_range(1.0, scale[i]);
                let anchor = light_pyramid(&frames[reference].linear, 1.0, lo, hi);
                let other = light_pyramid(&frame.linear, scale[i], lo, hi);
                match align_light(&anchor, &other) {
                    Some(warp) => {
                        warps[i] = warp;
                        notes.shifts[i] = warp.centre_shift(w, h);
                    }
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
    notes.warps = warps.clone();

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
        let warp = warps[j];
        let src_px = frame.linear.px();
        for y in 0..h {
            for x in 0..w {
                let Some((sx, sy)) = warp.source(x, y, w, h) else { continue };
                let dst = (y * w + x) * 3;
                let px = src_px[sy * w + sx];
                let level = px[0].max(px[1]).max(px[2]);
                let here = signal(level) * snr[j];
                if here > 0.0 {
                    let room = frame.headroom_at(sy * w + sx, px);
                    for c in 0..3 {
                        let trust = here * room[c];
                        if trust > 0.0 {
                            weight[dst + c] += trust;
                            sum[dst + c] += px[c] * scale[j] * trust;
                        }
                    }
                }
                // Whatever happens, remember the frame that came closest to a
                // usable reading, so no pixel is left without a value. Where
                // every frame clipped, that is the one that clipped least.
                let full = frame.fullness(sy * w + sx, px);
                let score = -(level - 0.35).abs() - if full >= CEILING { full } else { 0.0 };
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

    // Handed on in the darkest frame's light, so that 1 is its ceiling, as in
    // any other frame: development reads a value at the top as clipped and
    // exposes around it, and the highlights the darker frames held are not.
    let darkest = scale.iter().cloned().fold(1.0f32, f32::max);
    if darkest > 1.0 {
        let to_darkest = 1.0 / darkest;
        for v in out.d.iter_mut() {
            *v *= to_darkest;
        }
    }
    notes.units_stops = darkest.log2();
    Ok((out, notes))
}

/// What the merged frame is to be developed as: the reference frame's
/// metadata, but no one frame's exposure compensation. A bracket is shot
/// around the exposure that was wanted, so that is the middle of its
/// compensations; the reference's alone would brighten a -2/+2 bracket by
/// what the +2 frame was for.
pub fn merged_meta(frames: &[Frame], notes: &Notes) -> Meta {
    let mut meta = frames[notes.reference].meta.clone();
    let comps: Vec<f32> = frames.iter().map(|f| f.meta.exposure_comp).collect();
    let (lo, hi) = comps.iter().fold((f32::MAX, f32::MIN), |(l, h), c| (l.min(*c), h.max(*c)));
    meta.exposure_comp = if comps.is_empty() { 0.0 } else { 0.5 * (lo + hi) };
    meta.bracket_stops = notes.range_stops;
    meta.kept_above_stops = notes.units_stops;
    meta
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
            if !arbitrable_at(level, frames[reference].fullness(sy * w + sx, px)) {
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


// Alignment
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

/// Where a frame is read for each pixel of the reference: a homography, so
/// that a handheld bracket's turn and tilt are put back as well as its drift.
/// The frame is read at ((m0·x + m1·y + m2) / q, (m3·x + m4·y + m5) / q),
/// with q = m6·x + m7·y + 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warp {
    pub m: [f32; 8],
}

impl Warp {
    pub const IDENTITY: Warp = Warp { m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] };

    pub fn shift((dx, dy): (i32, i32)) -> Warp {
        Warp { m: [1.0, 0.0, dx as f32, 0.0, 1.0, dy as f32, 0.0, 0.0] }
    }

    /// Where the frame is read for reference position (x, y).
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let m = &self.m;
        let q = m[6] * x + m[7] * y + 1.0;
        ((m[0] * x + m[1] * y + m[2]) / q, (m[3] * x + m[4] * y + m[5]) / q)
    }

    /// The nearest pixel of a `w`×`h` frame this reference pixel reads, if
    /// the frame covers it. Nearest rather than interpolated: half a pixel
    /// of misplacement is below what the merge can see, and a resampled
    /// frame would no longer match its own sensor saturation.
    pub fn source(&self, x: usize, y: usize, w: usize, h: usize) -> Option<(usize, usize)> {
        let (sx, sy) = self.map(x as f32, y as f32);
        let (sx, sy) = (sx.round(), sy.round());
        if !(sx >= 0.0 && sy >= 0.0 && sx < w as f32 && sy < h as f32) {
            return None;
        }
        Some((sx as usize, sy as usize))
    }

    /// Whether the whole of a `w`×`h` frame stays on this side of the
    /// horizon the map has, as anything a camera moved to does: its
    /// denominator, which is linear, positive at all four corners.
    pub fn in_front(&self, w: f32, h: f32) -> bool {
        let q = |x: f32, y: f32| self.m[6] * x + self.m[7] * y + 1.0;
        [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().all(|(x, y)| q(*x, *y) > 0.0)
    }

    /// How far the frame is moved at the middle of a `w`×`h` frame.
    pub fn centre_shift(&self, w: usize, h: usize) -> (i32, i32) {
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let (sx, sy) = self.map(cx, cy);
        ((sx - cx).round() as i32, (sy - cy).round() as i32)
    }
}

/// How the alignment of a bracket cuts the frame up, and how far it looks.
///
/// The frame is cut into tiles to see how far each part of it moved, and a
/// homography fitted to them.
const TILES_X: usize = 6;
const TILES_Y: usize = 4;
/// The tiles start from the first pyramid level no wider than this, looking
/// this many of its pixels either side of where the whole frame moved.
const TILE_START_EDGE: usize = 400;
const TILE_SEARCH: i32 = 6;
/// A tile only counts when its best shift is clearly better than the others
/// it tried: a dark sky matches itself anywhere.
const TILE_DISTINCT: f32 = 0.7;
/// A tile that disagrees with the fitted map by more than this, in pixels of
/// the full frame, is left out of the fit and the map fitted again.
const TILE_OUTLIER_PX: f32 = 6.0;
/// The fewest tiles a homography is fitted to: four pin one down exactly, so
/// a wrong one would go unnoticed.
const MIN_TILES: usize = 6;
/// A turn is fitted only when a shift misses the tiles by more than this, in
/// pixels (root mean square; the tiles are measured at half size, to two
/// pixels), and the turn misses them by less than this share of that.
const TURN_WORTH_PX: f32 = 3.0;
const TURN_WORTH_SHARE: f32 = 0.6;
/// How far, as a share of the frame, a fitted map may move a corner beyond
/// what it does to the middle before it is not believed.
const WARP_LIMIT: f32 = 0.1;
/// Below this correlation at the best shift, the two frames are not showing
/// the same thing.
const MIN_CORRELATION: f32 = 0.5;
/// The light compared runs from a little above the noise to short of the
/// ceiling, in each frame's own values.
const ALIGN_FLOOR: f32 = 0.004;

/// The light two frames, `a` and `b` scaled into the reference's light,
/// both recorded clear of noise and clipping.
fn shared_range(a: f32, b: f32) -> (f32, f32) {
    ((ALIGN_FLOOR * a).max(ALIGN_FLOOR * b), (TRUSTED_HIGH * a).min(TRUSTED_HIGH * b))
}

/// A frame as the alignment compares it: log luminance in the reference's
/// light, held to `lo..hi`, at half size and smaller. Two frames of one
/// bracket look the same this way, whatever their exposures, apart from
/// what moved.
pub struct LightPyramid {
    levels: Vec<kit::Plane>,
}

fn light_pyramid(img: &Image, scale: f32, lo: f32, hi: f32) -> LightPyramid {
    let (w, h) = (img.w / 2, img.h / 2);
    let mut base = kit::Plane::new(w, h);
    let (llo, lhi) = (lo.max(1e-9).log2(), hi.max(lo * 1.01).log2());
    let px = img.px();
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                sum += kit::luminance_px(&px[(2 * y + dy) * img.w + 2 * x + dx]);
            }
            base.d[y * w + x] = (sum * 0.25 * scale).max(1e-9).log2().clamp(llo, lhi);
        }
    }
    let mut levels = vec![base];
    loop {
        let last = levels.last().unwrap();
        if last.w <= 64 || last.h <= 64 {
            break;
        }
        let next = kit::resize_plane(last, last.w / 2, last.h / 2);
        levels.push(next);
    }
    LightPyramid { levels }
}

/// The mean difference between `a` and `b` read `shift` away, over the
/// part of `rect` (x0, y0, x1, y1) of `a` both cover; none when that is less
/// than a quarter of it.
fn difference(a: &kit::Plane, b: &kit::Plane, (dx, dy): (i32, i32),
              rect: (usize, usize, usize, usize)) -> Option<f32> {
    let (xs, ys) = (overlap(a.w, dx), overlap(a.h, dy));
    let (x0, x1) = (rect.0.max(xs.start), rect.2.min(xs.end));
    let (y0, y1) = (rect.1.max(ys.start), rect.3.min(ys.end));
    if x1 <= x0 || y1 <= y0 || (x1 - x0) * (y1 - y0) * 4 < (rect.2 - rect.0) * (rect.3 - rect.1) {
        return None;
    }
    let mut sum = 0.0f64;
    for y in y0..y1 {
        let row_a = &a.d[y * a.w..][x0..x1];
        let sy = (y as i32 + dy) as usize;
        let sx = (x0 as i32 + dx) as usize;
        let row_b = &b.d[sy * b.w..][sx..sx + (x1 - x0)];
        sum += row_a.iter().zip(row_b).map(|(p, q)| (p - q).abs()).sum::<f32>() as f64;
    }
    Some((sum / ((x1 - x0) * (y1 - y0)) as f64) as f32)
}

/// The best shift within `r` of `around`, its difference, and the median
/// difference of all it tried.
fn search(a: &kit::Plane, b: &kit::Plane, around: (i32, i32), r: (i32, i32),
          rect: (usize, usize, usize, usize)) -> Option<((i32, i32), f32, f32)> {
    let mut best: Option<((i32, i32), f32)> = None;
    let mut all = Vec::new();
    for dy in -r.1..=r.1 {
        for dx in -r.0..=r.0 {
            let c = (around.0 + dx, around.1 + dy);
            let Some(d) = difference(a, b, c, rect) else { continue };
            all.push(d);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((c, d));
            }
        }
    }
    let (at, d) = best?;
    all.sort_by(f32::total_cmp);
    Some((at, d, all[all.len() / 2]))
}

/// How alike `a` and `b` are read `shift` apart, -1..1.
fn correlation(a: &kit::Plane, b: &kit::Plane, (dx, dy): (i32, i32)) -> f32 {
    let (mut n, mut sa, mut sb, mut saa, mut sbb, mut sab) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
    for y in overlap(a.h, dy) {
        let sy = (y as i32 + dy) as usize;
        for x in overlap(a.w, dx) {
            let p = a.d[y * a.w + x] as f64;
            let q = b.d[sy * b.w + (x as i32 + dx) as usize] as f64;
            n += 1.0;
            sa += p;
            sb += q;
            saa += p * p;
            sbb += q * q;
            sab += p * q;
        }
    }
    if n < 1.0 {
        return 0.0;
    }
    let cov = sab / n - sa * sb / n / n;
    let va = saa / n - sa * sa / n / n;
    let vb = sbb / n - sb * sb / n / n;
    if va <= 1e-12 || vb <= 1e-12 {
        return 0.0;
    }
    (cov / (va * vb).sqrt()) as f32
}

/// The map that lines `other` up with `anchor`, or none if they are not the
/// same scene. The whole frame first, searched across a quarter of it either
/// way, since a handheld bracket drifts by hundreds of pixels; then each tile
/// from there, for the turn and tilt between frames.
fn align_light(anchor: &LightPyramid, other: &LightPyramid) -> Option<Warp> {
    let depth = anchor.levels.len().min(other.levels.len());
    if depth == 0 || (0..depth).any(|l| anchor.levels[l].w != other.levels[l].w
                                     || anchor.levels[l].h != other.levels[l].h) {
        return None;
    }
    let whole = |p: &kit::Plane| (0, 0, p.w, p.h);
    let top = &anchor.levels[depth - 1];
    let (mut shift, _, _) = search(top, &other.levels[depth - 1], (0, 0),
                                   ((top.w / 4) as i32, (top.h / 4) as i32), whole(top))?;
    let start = (0..depth).find(|l| anchor.levels[*l].w <= TILE_START_EDGE).unwrap_or(depth - 1);
    let mut at_start = shift;
    for level in (0..depth - 1).rev() {
        shift = (shift.0 * 2, shift.1 * 2);
        let a = &anchor.levels[level];
        shift = search(a, &other.levels[level], shift, (1, 1), whole(a))?.0;
        if level == start {
            at_start = shift;
        }
    }
    if correlation(&anchor.levels[start], &other.levels[start], at_start) < MIN_CORRELATION {
        return None;
    }
    // The pyramid starts at half size.
    let fallback = Warp::shift((shift.0 * 2, shift.1 * 2));
    let (fw, fh) = (anchor.levels[0].w as f32 * 2.0, anchor.levels[0].h as f32 * 2.0);

    // (centre x, centre y, source x, source y), in full-frame pixels.
    let mut found = Vec::new();
    for ty in 0..TILES_Y {
        for tx in 0..TILES_X {
            let mut at = at_start;
            let mut usable = true;
            for level in (0..=start).rev() {
                let (a, b) = (&anchor.levels[level], &other.levels[level]);
                let rect = (tx * a.w / TILES_X, ty * a.h / TILES_Y,
                            (tx + 1) * a.w / TILES_X, (ty + 1) * a.h / TILES_Y);
                let r = if level == start { TILE_SEARCH } else { 1 };
                if level < start {
                    at = (at.0 * 2, at.1 * 2);
                }
                let Some((best, d, median)) = search(a, b, at, (r, r), rect) else {
                    usable = false;
                    break;
                };
                if level == start && d >= TILE_DISTINCT * median {
                    usable = false;
                    break;
                }
                at = best;
            }
            if usable {
                let cx = (tx as f32 + 0.5) * fw / TILES_X as f32;
                let cy = (ty as f32 + 0.5) * fh / TILES_Y as f32;
                found.push((cx, cy, cx + at.0 as f32 * 2.0, cy + at.1 as f32 * 2.0));
            }
        }
    }

    if found.is_empty() {
        return Some(fallback);
    }
    let (turned, keep) = fit_turn(&found, fw, fh);
    // Where the tiles agree the frame moved, which is the middle of what they
    // say: the same tiles the turn was fitted to, when it was.
    let kept: Vec<_> = found.iter().zip(&keep).filter(|(_, k)| **k || turned.is_none())
        .map(|(f, _)| *f).collect();
    let middle = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let dx = middle(kept.iter().map(|f| f.2 - f.0).collect());
    let dy = middle(kept.iter().map(|f| f.3 - f.1).collect());
    let shifted = Warp::shift((dx.round() as i32, dy.round() as i32));
    // The turn only where it explains the tiles clearly better than the shift
    // does. On a tripod the tiles disagree by their own measuring error alone,
    // which a homography fits too, and carries out to the corners several
    // times over, misplacing them by pixels.
    let Some(turned) = turned else { return Some(shifted) };
    let miss = |w: &Warp| {
        let sum: f32 = kept.iter().map(|(cx, cy, sx, sy)| {
            let (u, v) = w.map(*cx, *cy);
            (u - sx).powi(2) + (v - sy).powi(2)
        }).sum();
        (sum / kept.len() as f32).sqrt()
    };
    let (by_shift, by_turn) = (miss(&shifted), miss(&turned));
    Some(if by_shift > TURN_WORTH_PX && by_turn < TURN_WORTH_SHARE * by_shift { turned } else { shifted })
}

/// The turn and tilt the tiles (centre x, centre y, source x, source y) say a
/// `w`×`h` frame made, if they say one, and which tiles it was fitted to.
/// Fitted, and fitted again without the tile it fits worst, for as long as
/// that tile is out of line: a tile on something that moved, or on a
/// repeating pattern, is wrong in its own way.
fn fit_turn(found: &[(f32, f32, f32, f32)], w: f32, h: f32) -> (Option<Warp>, Vec<bool>) {
    let mut keep = vec![true; found.len()];
    while let Some(warp) = fit_homography(found, &keep) {
        // Tiles on one line, but for a stray, pin no turn down, and the fit
        // that matches them best sends that line off to infinity.
        if !warp.in_front(w, h) {
            break;
        }
        let worst = found.iter().enumerate().filter(|(k, _)| keep[*k])
            .map(|(k, (cx, cy, sx, sy))| {
                let (u, v) = warp.map(*cx, *cy);
                (k, (u - sx).hypot(v - sy))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match worst {
            Some((k, miss)) if miss > TILE_OUTLIER_PX => keep[k] = false,
            _ => return (believable(&warp, w, h).then_some(warp), keep),
        }
    }
    (None, keep)
}

/// Whether a fitted map is something a hand holding a camera could have done:
/// the frame's corners land within a tenth of its size of where a shift alone
/// would put them.
fn believable(warp: &Warp, w: f32, h: f32) -> bool {
    let (cx, cy) = warp.map(w / 2.0, h / 2.0);
    let (sx, sy) = (cx - w / 2.0, cy - h / 2.0);
    [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().all(|(x, y)| {
        let (u, v) = warp.map(*x, *y);
        (u - x - sx).abs() < WARP_LIMIT * w && (v - y - sy).abs() < WARP_LIMIT * h
    })
}

/// Whether tiles pin a turn down in both directions: two rows of the grid
/// with two tiles each, and two columns. Tiles along one row, and a stray or
/// two off it, leave the tilt across that row to the stray and its noise,
/// and a fit to them matches the tiles perfectly and the rest of the frame
/// not at all.
fn spread(tiles: &[(f32, f32, f32, f32)]) -> bool {
    let lines = |at: fn(&(f32, f32, f32, f32)) -> f32| {
        let mut count = std::collections::HashMap::new();
        for t in tiles {
            *count.entry(at(t).to_bits()).or_insert(0) += 1;
        }
        count.values().filter(|n| **n >= 2).count() >= 2
    };
    lines(|t| t.1) && lines(|t| t.0)
}

/// The homography that best carries the tiles' centres to where they were
/// found, by least squares; none if the tiles kept cannot pin one down.
fn fit_homography(found: &[(f32, f32, f32, f32)], keep: &[bool]) -> Option<Warp> {
    let kept: Vec<_> = found.iter().zip(keep).filter(|(_, k)| **k).map(|(f, _)| *f).collect();
    if kept.len() < MIN_TILES || !spread(&kept) {
        return None;
    }
    // In coordinates centred on the tiles and about a unit across, for a
    // well-conditioned system; the same normalisation both sides.
    let n = kept.len() as f64;
    let mx = kept.iter().map(|f| f.0 as f64).sum::<f64>() / n;
    let my = kept.iter().map(|f| f.1 as f64).sum::<f64>() / n;
    let k = kept.iter().map(|f| ((f.0 as f64 - mx).abs()).max((f.1 as f64 - my).abs()))
        .fold(1.0f64, f64::max);
    let mut ata = [[0f64; 8]; 8];
    let mut atb = [0f64; 8];
    for (cx, cy, tx, ty) in &kept {
        let (x, y) = ((*cx as f64 - mx) / k, (*cy as f64 - my) / k);
        let (u, v) = ((*tx as f64 - mx) / k, (*ty as f64 - my) / k);
        for (row, rhs) in [([x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u], u),
                           ([0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v], v)] {
            for i in 0..8 {
                for j in 0..8 {
                    ata[i][j] += row[i] * row[j];
                }
                atb[i] += row[i] * rhs;
            }
        }
    }
    let h = solve8(ata, atb)?;
    // Back to pixels: H = T⁻¹ · Hn · T, with T(p) = (p − m) / k.
    let t = [[1.0 / k, 0.0, -mx / k], [0.0, 1.0 / k, -my / k], [0.0, 0.0, 1.0]];
    let ti = [[k, 0.0, mx], [0.0, k, my], [0.0, 0.0, 1.0]];
    let hn = [[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]];
    let mul = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| {
        let mut c = [[0f64; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                c[i][j] = (0..3).map(|l| a[i][l] * b[l][j]).sum();
            }
        }
        c
    };
    let full = mul(mul(ti, hn), t);
    let z = full[2][2];
    if z.abs() < 1e-12 {
        return None;
    }
    let m = [full[0][0], full[0][1], full[0][2], full[1][0], full[1][1], full[1][2],
             full[2][0], full[2][1]].map(|v| (v / z) as f32);
    m.iter().all(|v| v.is_finite()).then_some(Warp { m })
}

/// Solve an 8×8 linear system by elimination with partial pivoting.
fn solve8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let pivot = (col..8).max_by(|i, j| a[*i][col].abs().total_cmp(&a[*j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..8 {
            let f = a[row][col] / a[col][col];
            for j in col..8 {
                a[row][j] -= f * a[col][j];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0f64; 8];
    for row in (0..8).rev() {
        let s: f64 = (row + 1..8).map(|j| a[row][j] * x[j]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
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
                saturation: Vec::new(),
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

    /// A scene with texture at every scale, seen two stops apart through a
    /// camera that drifted by `shift` and turned by `degrees` about the
    /// middle, clipping as a sensor does: the frame shows at p what the
    /// reference shows at the point the frame's warp reads p from.
    fn handheld(stops: f32, shift: (f32, f32), degrees: f32) -> Image {
        let (w, h) = (768usize, 512usize);
        let scene = |x: f32, y: f32| {
            let mut r = 0.01 * 2f32.powf(6.0 * x / w as f32);
            r *= 1.0 + 0.7 * (x * 0.05).sin() * (y * 0.07).cos();
            r *= 1.0 + 0.4 * (x * 0.31 + y * 0.17).sin();
            for (bx, by, rad) in [(150.0f32, 120.0f32, 40.0f32), (500.0, 300.0, 70.0),
                                  (650.0, 100.0, 30.0), (300.0, 400.0, 55.0)] {
                if (x - bx).hypot(y - by) < rad {
                    r *= 4.0;
                }
            }
            r
        };
        let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
        let (sin, cos) = degrees.to_radians().sin_cos();
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                // Undo the shift, then the turn.
                let (ux, uy) = (x as f32 - shift.0 - cx, y as f32 - shift.1 - cy);
                let (sx, sy) = (cos * ux + sin * uy + cx, -sin * ux + cos * uy + cy);
                let seen = (scene(sx, sy) * stops.exp2()).min(1.0);
                img.d[(y * w + x) * 3..][..3].copy_from_slice(&[seen; 3]);
            }
        }
        img
    }

    fn line_up(reference: &Image, other: &Image, stops: f32) -> Option<Warp> {
        let scale = (-stops).exp2();
        let (lo, hi) = shared_range(1.0, scale);
        align_light(&light_pyramid(reference, 1.0, lo, hi), &light_pyramid(other, scale, lo, hi))
    }

    /// Where a warp puts each corner of the frame, against where it should.
    fn worst_corner(warp: &Warp, truth: impl Fn(f32, f32) -> (f32, f32)) -> f32 {
        [(40.0, 40.0), (728.0, 40.0), (40.0, 472.0), (728.0, 472.0), (384.0, 256.0)].iter()
            .map(|(x, y)| {
                let (u, v) = warp.map(*x, *y);
                let (tu, tv) = truth(*x, *y);
                (u - tu).hypot(v - tv)
            })
            .fold(0.0, f32::max)
    }

    /// A handheld bracket drifts by far more than a few pixels between frames.
    #[test]
    fn a_frame_far_out_of_place_is_found() {
        let reference = handheld(0.0, (0.0, 0.0), 0.0);
        let other = handheld(2.0, (110.0, -70.0), 0.0);
        let warp = line_up(&reference, &other, 2.0).expect("the same scene");
        let miss = worst_corner(&warp, |x, y| (x + 110.0, y - 70.0));
        assert!(miss < 2.5, "should read the frame where it moved to, missed by {miss} px");
    }

    /// Six tiles along one row and one stray, as two frames of a real
    /// bracket left them: the homography that fits them best sends that row
    /// off to infinity, which once brought the whole merge down. No turn is
    /// read from them, and the frame is lined up by its shift.
    #[test]
    fn tiles_along_one_row_say_no_turn() {
        let found = [(454.7, 454.0, 344.7, 564.0), (1364.0, 454.0, 1270.0, 564.0),
                     (454.7, 1362.0, 466.7, 1336.0), (1364.0, 1362.0, 1366.0, 1352.0),
                     (2273.3, 1362.0, 2277.3, 1352.0), (454.7, 2270.0, 460.7, 2258.0),
                     (1364.0, 2270.0, 1368.0, 2260.0), (2273.3, 2270.0, 2277.3, 2260.0),
                     (3182.7, 2270.0, 3186.7, 2262.0), (4092.0, 2270.0, 4098.0, 2262.0),
                     (5001.3, 2270.0, 5009.3, 2264.0), (454.7, 3178.0, 460.7, 3166.0)];
        let keep = [false, true, false, false, false, true, true, true, true, true, true, false];
        assert!(fit_homography(&found, &keep).is_none(), "one row and a stray");
        let (turned, _) = fit_turn(&found, 5456.0, 3632.0);
        assert!(turned.is_none(), "{:?}", turned.map(|w| w.m));
    }

    /// And turns as it drifts, which a single shift cannot put back at the
    /// corners.
    #[test]
    fn a_turned_frame_is_put_back_corner_to_corner() {
        let reference = handheld(0.0, (0.0, 0.0), 0.0);
        let other = handheld(-2.0, (-40.0, 25.0), 1.5);
        let warp = line_up(&reference, &other, -2.0).expect("the same scene");
        let (cx, cy) = (384.0, 256.0);
        let (sin, cos) = 1.5f32.to_radians().sin_cos();
        // The frame shows at p what the reference shows at turn⁻¹(p − shift),
        // so the reference's x is read from the frame at turn(x) + shift.
        let truth = |x: f32, y: f32| {
            let (ux, uy) = (x - cx, y - cy);
            (cos * ux - sin * uy + cx - 40.0, sin * ux + cos * uy + cy + 25.0)
        };
        let miss = worst_corner(&warp, truth);
        let shift_only = worst_corner(&Warp::shift(warp.centre_shift(768, 512)), truth);
        assert!(miss < 2.5, "should follow the turn to the corners, missed by {miss} px");
        assert!(shift_only > 5.0, "the test needs a turn a shift cannot absorb: {shift_only}");
    }

    /// Lining up still has to be able to say "these are not the same picture".
    #[test]
    fn lining_up_refuses_two_different_scenes() {
        let reference = handheld(0.0, (0.0, 0.0), 0.0);
        let mut other = Image::new(reference.w, reference.h);
        let mut state = 0x243F6A88u32;
        for v in other.d.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *v = 0.3 * state as f32 / u32::MAX as f32;
        }
        assert_eq!(line_up(&reference, &other, 0.0), None);
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

    /// Where the sensor clipped, highlight reconstruction leaves a value that
    /// looks like any other; the sensor's own word is what tells them apart.
    #[test]
    fn what_the_sensor_clipped_is_not_trusted_however_it_looks() {
        let (w, h) = (64usize, 48usize);
        // The left half clipped on the sensor in the bright frame and was
        // filled in at 0.5; the dark frame, two stops down, saw 0.2 there,
        // which is 0.8 in the bright frame's light.
        let shot = |left: f32, right: f32, full: u8, shutter: f32| {
            let mut linear = Image::new(w, h);
            let mut saturation = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    let v = if x < w / 2 { left } else { right };
                    linear.d[(y * w + x) * 3..][..3].fill(v);
                    saturation[y * w + x] = if x < w / 2 { full } else { 100 };
                }
            }
            Frame { name: String::new(), linear, meta: meta(shutter, 100.0), exif: Vec::new(),
                    saturation }
        };
        let frames = vec![shot(0.2, 0.1, 60, 0.0025), shot(0.5, 0.4, 255, 0.01)];
        let (merged, notes) = merge(&frames, Options { align: false, deghost: Some(0.0) }).unwrap();

        // In the darkest frame's light, so the dark frame's values come back.
        assert!((notes.units_stops - 2.0).abs() < 0.01, "{notes:?}");
        let at = |x: usize| merged.d[(h / 2 * w + x) * 3];
        assert!((at(8) - 0.2).abs() < 0.005, "the reconstructed guess was used: {}", at(8));
        assert!((at(w - 8) - 0.1).abs() < 0.005, "the unclipped half went wrong: {}", at(w - 8));
    }

    /// A -2/+2 bracket was shot around the exposure wanted, not the +2 one.
    #[test]
    fn a_merged_frame_is_developed_as_the_bracket_meant() {
        let mut frames = vec![shot("a", 0, 0.0025, "", 35.0), shot("b", 0, 0.01, "", 35.0),
                              shot("c", 0, 0.04, "", 35.0)];
        for (f, comp) in frames.iter_mut().zip([-2.0, 0.0, 2.0]) {
            f.meta.exposure_comp = comp;
        }
        frames[1].meta.exposure_comp = 1.0;
        let notes = Notes { reference: 1, range_stops: 4.0, units_stops: 2.0, ..Notes::default() };
        let meta = merged_meta(&frames, &notes);
        assert_eq!(meta.exposure_comp, 0.0, "the middle of the bracket");
        assert_eq!((meta.bracket_stops, meta.kept_above_stops), (4.0, 2.0));
        assert_eq!(meta.shutter_s, frames[1].meta.shutter_s, "otherwise the reference's");
    }

    /// What a frame of the bracket held, a merged bracket's development keeps:
    /// nothing the darkest frame recorded under its ceiling comes out clipped,
    /// and nothing the brightest recorded clear of its noise comes out black,
    /// in the preview and in a piece of the frame looked at at 100 %.
    #[test]
    fn a_merged_bracket_keeps_what_its_frames_held() {
        let frames = bracket((0, 0));
        let (darkest, brightest) = (&frames[0].linear, &frames[2].linear);
        let (image, notes) = merge(&frames, Options { align: false, deghost: Some(0.0) })
            .expect("merge");
        let mut dev = crate::develop::Development::from_frame(
            "merged", image, merged_meta(&frames, &notes), Vec::new(), None);
        let settings = crate::grade::Settings::default();
        let code = |v: f32| kit::srgb_encode_scalar(v) * 255.0;
        let check = |out: &Image, x0: usize, y0: usize| {
            let (mut held, mut seen) = (0, 0);
            for y in 0..out.h {
                for x in 0..out.w {
                    let px = out.px()[y * out.w + x];
                    let i = (y + y0) * darkest.w + x + x0;
                    if darkest.px()[i][0] < 0.9 {
                        held += 1;
                        assert!(px.iter().all(|v| code(*v) < 254.5),
                                "held at {x},{y} came out clipped: {px:?}");
                    }
                    if brightest.px()[i][0] > 0.03 {
                        seen += 1;
                        assert!(code(kit::luminance_px(&px)) >= 8.0,
                                "seen at {x},{y} came out black: {px:?}");
                    }
                }
            }
            assert!(held > 100 && seen > 100, "the scene should test both: {held} {seen}");
        };
        let (preview, _) = dev.render(&settings, Some(128), true, None);
        assert_eq!((preview.w, preview.h), (darkest.w, darkest.h));
        check(&preview, 0, 0);
        let piece = dev.render_region(&settings, (0.6, 0.5), (48, 40), None);
        check(&piece.image, piece.x, piece.y);
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
            saturation: Vec::new(),
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

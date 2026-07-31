//! Scene analysis: every number the pipeline needs is measured here.

use crate::ops::{self, Image, Plane};
use serde_json::{json, Map, Value};

pub const EPS: f32 = 1e-6;

/// Scene light dim enough that nothing survives to the exported file.
pub const DISPLAY_FLOOR: f32 = 0.016;

/// How much of a frame may be given up to black before the exposure is treated
/// as too dark rather than deliberately dark.
pub const CRUSH_BUDGET: f32 = 0.10;

/// The most the rescue may add on top of the soft limit. Bounded because
/// lifting shadows lifts their noise with them.
pub const MAX_RESCUE_EV: f32 = 3.5;

/// Oklab lightness that still reaches the first code value of an 8-bit file.
/// Below this a pixel is black however much detail it holds.
pub const BLACK_LIGHTNESS: f32 = 0.0534;

const THIRD_STOP: f32 = 1.259_921_f32;

/// Smoothly saturate at ±limit instead of clipping hard.
pub fn soft_limit(x: f32, limit: f32) -> f32 {
    limit * (x / limit).tanh()
}

fn round_to(v: f32, places: i32) -> f64 {
    let f = 10f64.powi(places);
    ((v as f64) * f).round() / f
}

// -------------------------------------------------------------------------
// white balance
// -------------------------------------------------------------------------

/// Refine the camera's white balance with a shades-of-grey estimate.
pub fn auto_white_balance(thumb: &Image, strength: f32, limit_ev: f32) -> ([f32; 3], Value) {
    let y = ops::luminance(thumb);
    let pct = ops::percentiles(&y.d, &[3.0, 97.0]);
    let (lo, hi) = (pct[0], pct[1]);

    let mut sums = [0f64; 3];
    let mut count = 0usize;
    let p = 6.0f32;
    for i in 0..thumb.w * thumb.h {
        let px = &thumb.d[i * 3..i * 3 + 3];
        let maxc = px[0].max(px[1]).max(px[2]);
        if y.d[i] > lo.max(EPS) && y.d[i] < hi && maxc < 0.98 {
            for c in 0..3 {
                sums[c] += px[c].max(EPS).powf(p) as f64;
            }
            count += 1;
        }
    }
    if count < 64 {
        return ([1.0, 1.0, 1.0], json!({"applied": false}));
    }
    let mut est = [0f32; 3];
    for c in 0..3 {
        est[c] = ((sums[c] / count as f64).powf(1.0 / p as f64) as f32).max(EPS);
    }
    let mean = (est[0] + est[1] + est[2]) / 3.0;
    let mut log_gain = [0f32; 3];
    for c in 0..3 {
        log_gain[c] = (mean / est[c]).log2();
    }
    let lw = [0.2126f32, 0.7152, 0.0722];
    let weighted: f32 = (0..3).map(|c| log_gain[c] * lw[c]).sum::<f32>() / lw.iter().sum::<f32>();
    let mut gains = [0f32; 3];
    for c in 0..3 {
        // Restraint belongs in the soft limit, not in a flat multiplier.
        log_gain[c] = soft_limit((log_gain[c] - weighted) * strength, limit_ev);
        gains[c] = log_gain[c].exp2();
    }
    (
        gains,
        json!({"applied": true,
               "gains": [round_to(gains[0],4), round_to(gains[1],4), round_to(gains[2],4)],
               "shift_ev": [round_to(log_gain[0],3), round_to(log_gain[1],3), round_to(log_gain[2],3)]}),
    )
}

// -------------------------------------------------------------------------
// exposure
// -------------------------------------------------------------------------

/// Match the log-average luminance to middle grey, partially, with a highlight
/// guard that gives way when the raw was already clipped.
pub fn auto_exposure(thumb: &Image, target_key: f32, strength: f32, limit_ev: f32) -> (f32, Value) {
    let y = ops::luminance(thumb);
    let floor = ops::percentile(&y.d, 1.0).max(1e-5);
    let sample: Vec<f32> = y.d.iter().cloned().filter(|v| *v > floor).collect();
    if sample.len() < 64 {
        return (1.0, json!({"applied": false}));
    }
    let log_mean: f64 = sample.iter().map(|v| v.log2() as f64).sum::<f64>() / sample.len() as f64;
    let key = (log_mean as f32).exp2();
    let delta = soft_limit((target_key / key.max(EPS)).log2() * strength, limit_ev);
    let mut gain = delta.exp2();

    // --- rescue, rather than restraint ------------------------------------
    // The soft limit above keeps a deliberately dark frame dark, which is right
    // for a night portrait and wrong for a frame that is simply underexposed.
    let buried = |g: f32| {
        y.d.iter().filter(|v| **v * g < DISPLAY_FLOOR).count() as f32 / y.d.len() as f32
    };
    let mut rescue_ev = 0.0f32;
    while rescue_ev < MAX_RESCUE_EV && buried(gain) > CRUSH_BUDGET {
        gain *= THIRD_STOP;
        rescue_ev += 1.0 / 3.0;
    }

    let clipped = y.d.iter().filter(|v| **v >= 0.99).count() as f32 / y.d.len() as f32;
    let mut guard: Option<f32> = None;
    let mut waived = false;
    if clipped < 0.005 {
        let hi = ops::percentile(&y.d, 99.5);
        let cap = 1.20 / hi.max(EPS);
        if cap < gain {
            guard = Some(gain);
            // The guard and the rescue want opposite things, and on a frame
            // that needed rescuing the guard used to win and undo it.
            let floor = if rescue_ev > 0.0 { gain } else { 0.0 };
            gain = cap.max(floor);
            waived = gain > cap;
        }
    }
    // The ceiling has to leave room for the rescue; the highlight guard above
    // is what stops it running away, not this clamp.
    gain = gain.clamp(0.125, 2f32.powf(limit_ev + MAX_RESCUE_EV));
    (
        gain,
        json!({"applied": true, "key": round_to(key, 5), "gain_ev": round_to(gain.log2(), 3),
               "highlight_guard_ev": guard.map(|g| round_to((g / gain).log2(), 3)),
               "highlight_guard_waived": waived,
               "rescue_ev": round_to(rescue_ev, 2),
               "buried_after": round_to(buried(gain), 4),
               "clipped_fraction": round_to(clipped, 5)}),
    )
}

// -------------------------------------------------------------------------
// dynamic range
// -------------------------------------------------------------------------

/// How hard to squeeze the scene's range into the display's.
pub fn auto_tone_compression(thumb: &Image, comfortable_stops: f32, floor: f32) -> (f32, Value) {
    let y = ops::luminance(thumb);
    let logs: Vec<f32> = y.d.iter().map(|v| v.max(EPS).log2()).collect();
    let p = ops::percentiles(&logs, &[2.0, 99.5]);
    let stops = p[1] - p[0];
    let factor = (comfortable_stops / stops.max(EPS)).clamp(floor, 1.0);
    (
        factor,
        json!({"scene_stops": round_to(stops, 2), "base_compression": round_to(factor, 3)}),
    )
}

// -------------------------------------------------------------------------
// display-referred tone
// -------------------------------------------------------------------------

/// Robust black/white point from percentiles, moved only part of the way.
pub fn auto_levels(lightness: &Plane, strength: f32, clip_pct: f64) -> (f32, f32, Value) {
    let p = ops::percentiles(&lightness.d, &[clip_pct, 100.0 - clip_pct]);
    let wanted = p[0] * strength;
    let white = p[1] + (1.0 - p[1]) * strength;

    // A black point read off a percentile assumes the shadows are spread out.
    let budget_level = ops::percentile(&lightness.d, (CRUSH_BUDGET * 100.0) as f64);
    let ceiling = (budget_level - BLACK_LIGHTNESS * white) / (1.0 - BLACK_LIGHTNESS);
    let black = wanted.min(ceiling).max(0.0);

    if white - black < 0.25 {
        return (0.0, 1.0, json!({"applied": false}));
    }
    (
        black,
        white,
        json!({"applied": true, "black": round_to(black, 4), "white": round_to(white, 4),
               "wanted_black": round_to(wanted, 4),
               "stretch": round_to(1.0 / (white - black), 3)}),
    )
}

/// How much of a look's curve every frame gets, whatever it measures.
const CURVE_FLOOR: f32 = 0.40;

/// How much curve the tone map's own compression is allowed to ask for.
const SQUEEZE_TO_CURVE: f32 = 1.72;

/// S-curve strength and its pivot.
pub fn auto_contrast(lightness: &Plane, target_spread: f32, max_amount: f32, compression: f32)
    -> (f32, f32, Value) {
    let spread = ops::std_dev(&lightness.d);
    // How flat the histogram is.
    let flat = ((target_spread - spread) / target_spread).clamp(0.0, 1.0);
    // How much slope the tone map took out on the way here.
    let squeezed = ((1.0 - compression) * SQUEEZE_TO_CURVE).clamp(0.0, 1.0);
    let want = CURVE_FLOOR + (1.0 - CURVE_FLOOR) * flat.max(squeezed);
    let amount = want * max_amount;
    let pivot = ops::median(&lightness.d).clamp(0.2, 0.8);
    (
        amount,
        pivot,
        json!({"spread": round_to(spread, 4), "s_curve": round_to(amount, 3),
               "pivot": round_to(pivot, 3), "flat": round_to(flat, 3),
               "squeezed": round_to(squeezed, 3)}),
    )
}

// -------------------------------------------------------------------------
// colour
// -------------------------------------------------------------------------

/// Saturation boost from how colourful the frame already is, measured on the
/// 80th percentile of chroma over well-lit pixels.
pub fn auto_vibrance(chroma: &Plane, lightness: &Plane, target_chroma: f32, max_boost: f32)
    -> (f32, Value) {
    let sel: Vec<f32> = (0..chroma.d.len())
        .filter(|i| lightness.d[*i] > 0.15 && lightness.d[*i] < 0.95)
        .map(|i| chroma.d[i])
        .collect();
    if sel.len() < 64 {
        return (1.0, json!({"applied": false}));
    }
    let reference = ops::percentile(&sel, 80.0);
    let boost = (target_chroma / reference.max(1e-4)).clamp(1.0, max_boost);
    (
        boost,
        json!({"applied": true, "chroma_p80": round_to(reference, 4), "boost": round_to(boost, 3)}),
    )
}

/// Noise level in [0,1].
pub fn estimate_noise(lightness: &Plane, iso: f32, prior: f32) -> (f32, Value) {
    noise_from_floor(noise_floor(lightness), iso, prior)
}

/// The noise floor of a frame: the local variation left where the picture is
/// flattest, which is the one place the signal cannot be mistaken for texture.
pub fn noise_floor(lightness: &Plane) -> f32 {
    let blurred = ops::gaussian_blur(lightness, 1.0);
    let mut sq = Plane::new(lightness.w, lightness.h);
    for i in 0..sq.d.len() {
        let r = lightness.d[i] - blurred.d[i];
        sq.d[i] = r * r;
    }
    let local = ops::box_blur(&sq, 4);
    let rms: Vec<f32> = local.d.iter().map(|v| v.max(0.0).sqrt()).collect();
    ops::percentile(&rms, 8.0)
}

/// Turn a measured floor into the level the rest of the pipeline reasons
/// about, bounded by what the sensor and ISO make physically plausible.
pub fn noise_from_floor(floor: f32, iso: f32, prior: f32) -> (f32, Value) {
    let measured = (floor * 55.0).clamp(0.0, 1.0);
    let level = prior.max(measured.min(prior + 0.35)).clamp(0.0, 1.0);
    (
        level,
        json!({"iso": iso, "flat_area_noise": round_to(floor, 5), "measured": round_to(measured, 3),
               "prior": round_to(prior, 3), "noise_level": round_to(level, 3)}),
    )
}

// -------------------------------------------------------------------------
// report
// -------------------------------------------------------------------------

/// Everything the pipeline decided, keyed by stage name (insertion ordered,
/// like the Python dict the interface expects).
#[derive(Default, Clone, Debug)]
pub struct Report {
    pub order: Vec<String>,
    pub stages: Map<String, Value>,
}

impl Report {
    pub fn new() -> Self {
        Report::default()
    }
    pub fn add(&mut self, stage: &str, info: Value) {
        if !self.stages.contains_key(stage) {
            self.order.push(stage.to_string());
        }
        self.stages.insert(stage.to_string(), info);
    }
    pub fn get(&self, stage: &str) -> Value {
        self.stages.get(stage).cloned().unwrap_or(Value::Object(Map::new()))
    }
    pub fn to_json(&self) -> Value {
        let mut out = Map::new();
        for k in &self.order {
            out.insert(k.clone(), self.stages[k].clone());
        }
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(luminance: f32, spread: f32, w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for i in 0..w * h {
            // A gentle ramp, so percentiles have something to bite on.
            let v = luminance * (1.0 + spread * ((i % 97) as f32 / 97.0 - 0.5));
            for c in 0..3 {
                img.d[i * 3 + c] = v;
            }
        }
        img
    }

    /// A plane of a given lightness spread, to ask `auto_contrast` about.
    fn lightness(spread: f32, w: usize, h: usize) -> Plane {
        let mut p = Plane::new(w, h);
        for i in 0..w * h {
            // Two clumps at either end, which is what a wide but flat
            // histogram looks like: a dark subject against a bright sky.
            p.d[i] = if i % 2 == 0 { 0.5 - spread } else { 0.5 + spread };
        }
        p
    }

    /// Every frame gets a curve.
    #[test]
    fn a_contrasty_frame_still_gets_a_curve() {
        let wide = lightness(0.35, 64, 48);
        let (amount, _, info) = auto_contrast(&wide, 0.20, 0.45, 1.0);
        assert_eq!(info["flat"].as_f64().unwrap(), 0.0,
                   "this frame is past the target spread, so nothing is missing: {info}");
        assert!((amount - 0.45 * CURVE_FLOOR).abs() < 1e-4,
                "it should still get the floor, got {amount}");
    }

    /// A flat frame still gets more than a contrasty one, which is the part of
    /// the old behaviour worth keeping.
    #[test]
    fn a_flat_frame_gets_more_curve_than_a_wide_one() {
        let flat = auto_contrast(&lightness(0.02, 64, 48), 0.20, 0.45, 1.0).0;
        let wide = auto_contrast(&lightness(0.35, 64, 48), 0.20, 0.45, 1.0).0;
        assert!(flat > wide + 0.1, "flat {flat} should ask for much more than wide {wide}");
        assert!(flat <= 0.45, "nothing may exceed the look's own maximum, got {flat}");
    }

    /// The tone map squeezing a scene toward the middle is invisible to the
    /// spread, because it preserves the extremes that set it.
    #[test]
    fn a_squeezed_frame_gets_a_curve_its_histogram_cannot_ask_for() {
        let wide = lightness(0.35, 64, 48);
        let untouched = auto_contrast(&wide, 0.20, 0.45, 1.0).0;
        let squeezed = auto_contrast(&wide, 0.20, 0.45, 0.64).0;
        assert!(squeezed > untouched * 1.5,
                "ten stops pulled into a display should ask for far more curve: \
                 {squeezed} against {untouched}");
        assert!(squeezed <= 0.45, "still bounded by the look, got {squeezed}");
    }

    /// An underexposed frame has to be lifted past the usual restraint, or the
    /// part of it below the display's floor is lost for good.
    #[test]
    fn a_buried_frame_is_rescued() {
        let dark = frame(0.0004, 0.6, 120, 90);
        let (gain, info) = auto_exposure(&dark, 0.13, 0.85, 3.2);
        assert!(info["rescue_ev"].as_f64().unwrap() > 1.0,
                "an underexposed frame should be rescued: {info}");
        assert!(gain.log2() > 3.2,
                "the rescue should reach past the soft limit, got {} EV", gain.log2());
    }

    /// A properly exposed frame must come out exactly as it did before the
    /// rescue existed: the guard is for emergencies, not for everyone.
    #[test]
    fn a_well_exposed_frame_is_left_alone() {
        let ordinary = frame(0.13, 0.9, 120, 90);
        let (_, info) = auto_exposure(&ordinary, 0.13, 0.85, 3.2);
        assert_eq!(info["rescue_ev"].as_f64().unwrap(), 0.0,
                   "nothing to rescue here: {info}");
    }

    /// A dense band of shadow just above the black point is the case that used
    /// to lose half a frame to flat black.
    #[test]
    fn the_black_point_cannot_swallow_the_shadows() {
        // Two thirds of the frame piled into a narrow dark band.
        let mut dark = Plane::new(300, 100);
        for (i, v) in dark.d.iter_mut().enumerate() {
            *v = if i % 3 == 0 { 0.55 + (i % 7) as f32 * 0.01 }
                 else { 0.11 + (i % 5) as f32 * 0.002 };
        }
        let (black, white, info) = auto_levels(&dark, 0.6, 0.15);
        let survives = |l: f32| (l - black) / (white - black) > BLACK_LIGHTNESS;
        let lost = dark.d.iter().filter(|l| !survives(**l)).count() as f32 / dark.d.len() as f32;
        assert!(lost <= CRUSH_BUDGET + 0.02,
                "the black point pinned {:.1}% of the frame: {info}", lost * 100.0);
    }

    /// The floor is a property of the capture, so the same floor has to give
    /// the same answer whatever resolution asked for it.
    #[test]
    fn the_noise_level_follows_the_floor_not_the_scale() {
        let (a, _) = noise_from_floor(0.004, 200.0, 0.1);
        let (b, _) = noise_from_floor(0.004, 200.0, 0.1);
        assert_eq!(a, b);
        let (more, _) = noise_from_floor(0.008, 200.0, 0.1);
        assert!(more > a, "a noisier floor should read noisier");
    }
}

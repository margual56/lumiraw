//! Scene analysis: every number the pipeline needs is measured here.

use crate::ops::{self, Image, Plane};
use serde_json::{json, Map, Value};

pub const EPS: f32 = 1e-6;

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

    let clipped = y.d.iter().filter(|v| **v >= 0.99).count() as f32 / y.d.len() as f32;
    let mut guard: Option<f32> = None;
    if clipped < 0.005 {
        let hi = ops::percentile(&y.d, 99.5);
        let cap = 1.20 / hi.max(EPS);
        if cap < gain {
            guard = Some(gain);
            gain = cap;
        }
    }
    gain = gain.clamp(0.125, 8.0);
    (
        gain,
        json!({"applied": true, "key": round_to(key, 5), "gain_ev": round_to(gain.log2(), 3),
               "highlight_guard_ev": guard.map(|g| round_to((g / gain).log2(), 3)),
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
    let black = p[0] * strength;
    let white = p[1] + (1.0 - p[1]) * strength;
    if white - black < 0.25 {
        return (0.0, 1.0, json!({"applied": false}));
    }
    (
        black,
        white,
        json!({"applied": true, "black": round_to(black, 4), "white": round_to(white, 4),
               "stretch": round_to(1.0 / (white - black), 3)}),
    )
}

/// S-curve strength from how flat the histogram is, plus its pivot.
pub fn auto_contrast(lightness: &Plane, target_spread: f32, max_amount: f32) -> (f32, f32, Value) {
    let spread = ops::std_dev(&lightness.d);
    let amount = ((target_spread - spread) / target_spread).clamp(0.0, 1.0) * max_amount;
    let pivot = ops::median(&lightness.d).clamp(0.2, 0.8);
    (
        amount,
        pivot,
        json!({"spread": round_to(spread, 4), "s_curve": round_to(amount, 3),
               "pivot": round_to(pivot, 3)}),
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
    let blurred = ops::gaussian_blur(lightness, 1.0);
    let mut sq = Plane::new(lightness.w, lightness.h);
    for i in 0..sq.d.len() {
        let r = lightness.d[i] - blurred.d[i];
        sq.d[i] = r * r;
    }
    let local = ops::box_blur(&sq, 4);
    let rms: Vec<f32> = local.d.iter().map(|v| v.max(0.0).sqrt()).collect();
    let floor = ops::percentile(&rms, 8.0);
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

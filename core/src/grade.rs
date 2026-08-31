//! The grading pipeline: linear raw in, display-ready sRGB out.

use crate::analyze;
use crate::curve;
use crate::geometry::{stats_view_img, stats_view_plane, Framing, Rect};
use kit::{Image, Lab, Plane, Report, round_to, with};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const EPS: f32 = 1e-6;

/// The most the automatic white balance may move one channel, in stops.
pub const WB_LIMIT_EV: f32 = 0.8;

/// Taste knobs.  The *decisions* stay automatic; these scale them.
#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub wb_strength: f32,
    pub exposure_strength: f32,
    pub target_key: f32,
    pub comfortable_stops: f32,
    pub detail_boost: f32,
    pub levels_strength: f32,
    pub contrast_amount: f32,
    pub target_chroma: f32,
    pub max_boost: f32,
    pub sharpen: f32,
    pub denoise: bool,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            name: "natural".into(),
            wb_strength: 1.0,
            exposure_strength: 1.0,
            target_key: 0.13,
            comfortable_stops: 6.5,
            detail_boost: 1.15,
            levels_strength: 0.60,
            contrast_amount: 0.45,
            target_chroma: 0.078,
            max_boost: 1.55,
            sharpen: 0.55,
            denoise: true,
        }
    }
}

pub fn preset_by_name(name: &str) -> Preset {
    let base = Preset::default();
    match name {
        "punchy" => Preset {
            name: "punchy".into(),
            target_key: 0.145,
            comfortable_stops: 6.0,
            detail_boost: 1.25,
            levels_strength: 0.75,
            contrast_amount: 0.60,
            target_chroma: 0.095,
            max_boost: 1.8,
            sharpen: 0.75,
            ..base
        },
        "flat" => Preset {
            name: "flat".into(),
            wb_strength: 0.8,
            exposure_strength: 0.85,
            comfortable_stops: 8.0,
            detail_boost: 1.0,
            levels_strength: 0.35,
            contrast_amount: 0.2,
            target_chroma: 0.062,
            max_boost: 1.2,
            sharpen: 0.35,
            ..base
        },
        _ => base,
    }
}

/// Every automatic correction the user is allowed to switch off, in the order
/// the pipeline applies them.  The UI builds its toggle list straight from here.
pub const TOGGLES: [(&str, &str, &str); 13] = [
    ("lens_vignetting", "Vignetting", "Lens"),
    ("lens_distortion", "Distortion", "Lens"),
    ("lens_tca", "Chromatic aberration", "Lens"),
    ("white_balance", "White balance", "Colour"),
    ("exposure", "Exposure", "Tone"),
    ("tone_map", "Local tone mapping", "Tone"),
    ("levels", "Black & white point", "Tone"),
    ("contrast", "Contrast", "Tone"),
    ("vibrance", "Vibrance", "Colour"),
    ("denoise_chroma", "Colour noise", "Detail"),
    ("denoise_luma", "Luminance noise", "Detail"),
    ("refocus", "Focus recovery", "Detail"),
    ("sharpen", "Sharpening", "Detail"),
];

/// Everything the user chose.  `None` means "decide it automatically".
#[derive(Clone, Debug)]
pub struct Settings {
    pub framing: Framing,
    pub exposure_rect: Option<Rect>,
    pub exposure_bias: f32,
    pub wb_rect: Option<Rect>,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    /// Tone zones, -1..1 each, riding on top of `exposure_bias`.
    pub shadows: f32,
    pub midtones: f32,
    pub highlights: f32,
    /// How hard to pull a soft frame back, 0..1. Left at zero unless the
    /// frame was measured soft and the photographer asked for it.
    pub refocus: f32,
    /// Local contrast, -1..1: how much more or less of the fine structure the
    /// tone map keeps than it would by itself. Zero is what it decided.
    pub clarity: f32,
    /// Noise reduction, -1..1, on top of the amount measured for this frame.
    /// Zero is the measured amount; -1 turns it off, 1 is as much as it goes.
    pub denoise: f32,
    /// Graduated and radial filters, in fractions of the framed picture.
    pub filters: Vec<crate::local::Filter>,
    /// Healed spots, in fractions of the framed picture.
    pub spots: Vec<crate::local::Spot>,
    pub preset: String,
    /// The grade: a curve on all three channels and one each on red, green and
    /// blue.
    pub curves: curve::Stack,
    /// Eight hue bands, each with a hue, a saturation and a lightness. What
    /// curves cannot do: pick out one colour and leave the rest.
    pub mixer: crate::mixer::Mixer,
    /// How much of the loaded 3D table to apply, 0..1.
    pub lut: f32,
    /// The three things a curve cannot do, each 0..1. See `effects.rs`.
    pub monochrome: f32,
    pub vignette: f32,
    pub grain: f32,
    pub enabled: HashMap<String, bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            framing: Framing::default(),
            exposure_rect: None,
            exposure_bias: 0.0,
            wb_rect: None,
            temperature: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            shadows: 0.0,
            midtones: 0.0,
            highlights: 0.0,
            refocus: 0.0,
            clarity: 0.0,
            denoise: 0.0,
            filters: Vec::new(),
            spots: Vec::new(),
            preset: "natural".into(),
            curves: curve::Stack::identity(),
            mixer: crate::mixer::Mixer::default(),
            lut: 0.0,
            monochrome: 0.0,
            vignette: 0.0,
            grain: 0.0,
            enabled: HashMap::new(),
        }
    }
}

impl Settings {
    pub fn on(&self, key: &str) -> bool {
        *self.enabled.get(key).unwrap_or(&true)
    }
    pub fn preset_obj(&self) -> Preset {
        preset_by_name(&self.preset)
    }

    pub fn from_json(v: &Value) -> Settings {
        let mut s = Settings::default();
        let obj = match v.as_object() {
            Some(o) => o,
            None => return s,
        };
        let f = |k: &str| obj.get(k).and_then(|x| x.as_f64()).map(|x| x as f32);
        s.exposure_rect = obj.get("exposure_rect").and_then(rect_of);
        s.wb_rect = obj.get("wb_rect").and_then(rect_of);
        s.exposure_bias = f("exposure_bias").unwrap_or(0.0);
        s.temperature = f("temperature").unwrap_or(0.0);
        s.tint = f("tint").unwrap_or(0.0);
        s.vibrance = f("vibrance").unwrap_or(0.0);
        s.shadows = f("shadows").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.midtones = f("midtones").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.highlights = f("highlights").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.refocus = f("refocus").unwrap_or(0.0).clamp(0.0, 1.0);
        s.clarity = f("clarity").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.denoise = f("denoise").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.filters = crate::local::filters_from_json(obj.get("filters"));
        s.spots = crate::local::spots_from_json(obj.get("spots"));
        if let Some(p) = obj.get("preset").and_then(|x| x.as_str()) {
            s.preset = p.to_string();
        }
        s.curves = curve::Stack::from_json(obj.get("curves"));
        s.mixer = crate::mixer::Mixer::from_json(obj.get("mixer"));
        s.lut = f("lut").unwrap_or(0.0).clamp(0.0, 1.0);
        s.monochrome = f("monochrome").unwrap_or(0.0).clamp(0.0, 1.0);
        s.vignette = f("vignette").unwrap_or(0.0).clamp(0.0, 1.0);
        s.grain = f("grain").unwrap_or(0.0).clamp(0.0, 1.0);
        if let Some(fr) = obj.get("framing").and_then(|x| x.as_object()) {
            let g = |k: &str| fr.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
            s.framing.angle = g("angle");
            s.framing.perspective_v = g("perspective_v");
            s.framing.perspective_h = g("perspective_h");
            s.framing.auto_fit = fr.get("auto_fit").and_then(|x| x.as_bool()).unwrap_or(true);
            s.framing.crop = fr.get("crop").and_then(rect_of);
        }
        if let Some(en) = obj.get("enabled").and_then(|x| x.as_object()) {
            for (k, v) in en {
                if let Some(b) = v.as_bool() {
                    s.enabled.insert(k.clone(), b);
                }
            }
        }
        s
    }
}

/// A rectangle sent as `[x0, y0, x1, y1]`; anything shorter is no rectangle.
fn rect_of(v: &Value) -> Option<Rect> {
    let a = v.as_array()?;
    if a.len() < 4 {
        return None;
    }
    let g = |i: usize| a[i].as_f64().unwrap_or(0.0) as f32;
    Some((g(0), g(1), g(2), g(3)))
}

// Hue of average skin in Oklab, so the saturation stage can leave faces alone.
fn skin_hue() -> f32 {
    let r = kit::srgb_decode_scalar(0.76);
    let g = kit::srgb_decode_scalar(0.57);
    let b = kit::srgb_decode_scalar(0.46);
    let (_, a, bb) = kit::rgb_to_oklab_px(r, g, b);
    bb.atan2(a)
}

// -------------------------------------------------------------------------
// region sampling
// -------------------------------------------------------------------------

/// Pixels inside a normalised rectangle, with blown ones dropped.
pub fn sample_rect(img: &Image, rect: Rect) -> Option<Vec<[f32; 3]>> {
    let (w, h) = (img.w as f32, img.h as f32);
    let (x, y, rw, rh) = rect;
    let x0 = (x.clamp(0.0, 1.0) * w) as usize;
    let y0 = (y.clamp(0.0, 1.0) * h) as usize;
    let x1 = ((x + rw).clamp(0.0, 1.0) * w) as usize;
    let y1 = ((y + rh).clamp(0.0, 1.0) * h) as usize;
    let (x0, x1) = (x0.min(x1), x0.max(x1).max(x0 + 1).min(img.w));
    let (y0, y1) = (y0.min(y1), y0.max(y1).max(y0 + 1).min(img.h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut all = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for yy in y0..y1 {
        all.extend_from_slice(&img.px()[yy * img.w + x0..yy * img.w + x1]);
    }
    let keep: Vec<[f32; 3]> =
        all.iter().copied().filter(|px| px[0].max(px[1]).max(px[2]) < 0.99).collect();
    Some(if keep.len() >= 16 { keep } else { all })
}

// -------------------------------------------------------------------------
// scene-linear stages
// -------------------------------------------------------------------------

/// White balance, on the frame and on its measuring thumbnail alike.
pub fn apply_white_balance(img: &mut Image, thumb: &mut Image, s: &Settings, p: &Preset,
                           report: &mut Report) {
    if !s.on("white_balance") {
        report.skipped("white balance", "switched_off", json!({}));
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    if let Some(patch) = s.wb_rect.and_then(|r| sample_rect(img, r)).filter(|p| p.len() >= 4) {
        let n = patch.len() as f32;
        let mut mean = [0f32; 3];
        for px in &patch {
            for c in 0..3 {
                mean[c] += px[c];
            }
        }
        let mean = mean.map(|m| (m / n).max(EPS));
        let avg = (mean[0] + mean[1] + mean[2]) / 3.0;
        let gains = [avg / mean[0], avg / mean[1], avg / mean[2]];
        let norm = kit::luminance_px(&gains);
        let gains = gains.map(|g| g / norm);
        report.add("white balance", json!({"applied": true, "source": "selection",
            "gains": gains.map(|g| round_to(g, 4)), "patch_px": patch.len()}));
        adapt(img, gains);
        adapt(thumb, gains);
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    // The tuned preset, not the named one: the capture decides how far the
    // automatic balance may be trusted (see `profile::tune`).
    let (gains, info) = analyze::auto_white_balance(thumb, p.wb_strength, WB_LIMIT_EV);
    let info = with(info, json!({"source": "automatic", "strength": round_to(p.wb_strength, 2)}));
    let applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
    report.add("white balance", info);
    if applied {
        adapt(img, gains);
        adapt(thumb, gains);
    }
    apply_temp_tint(img, thumb, s, report);
}

fn apply_temp_tint(img: &mut Image, thumb: &mut Image, s: &Settings, report: &mut Report) {
    if s.temperature.abs() < 1e-3 && s.tint.abs() < 1e-3 {
        return;
    }
    let gains = temp_tint_gains(s.temperature, s.tint);
    report.add("temp/tint", json!({"applied": true, "temperature": round_to(s.temperature,3),
        "tint": round_to(s.tint,3), "gains": gains.map(|g| round_to(g, 4))}));
    adapt(img, gains);
    adapt(thumb, gains);
}

/// The channel gains a temperature and tint slider ask for, centred so the
/// colour moves and the luminance holds. Local filters use the same ones.
pub fn temp_tint_gains(temperature: f32, tint: f32) -> [f32; 3] {
    let log_gain = [
        0.35 * temperature - 0.125 * tint,
        0.25 * tint,
        -0.35 * temperature - 0.125 * tint,
    ];
    let weighted = kit::luminance_px(&log_gain);
    log_gain.map(|g| (g - weighted).exp2())
}

/// The white balance correction that per-channel `gains` describe, applied as a
/// change of light rather than as three volume knobs.
pub fn adapt(img: &mut Image, gains: [f32; 3]) {
    if gains.iter().all(|g| (g - 1.0).abs() < 1e-6) {
        return;
    }
    let white = [1.0 / gains[0].max(1e-6), 1.0 / gains[1].max(1e-6), 1.0 / gains[2].max(1e-6)];
    kit::bradford_adaptation(white).apply_to(&mut img.d);
}

pub fn apply_exposure(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                      report: &mut Report) -> f32 {
    if !s.on("exposure") {
        report.skipped("exposure", "switched_off", json!({}));
        let gain = s.exposure_bias.exp2();
        img.scale(gain);
        return gain;
    }
    if let Some(patch) = s.exposure_rect.and_then(|r| sample_rect(img, r)) {
        // Geometric mean: a few bright specks in the selection should not
        // drag the whole frame down.
        let log_sum: f64 = patch
            .iter()
            .map(|px| kit::luminance_px(px).max(EPS).log2() as f64)
            .sum();
        let level = ((log_sum / patch.len() as f64) as f32).exp2();
        let mut gain = (p.target_key / level).clamp(2f32.powi(-6), 2f32.powi(8));
        gain *= s.exposure_bias.exp2();
        report.add("exposure", json!({"applied": true, "source": "selection",
            "patch_level": round_to(level, 5), "gain_ev": round_to(gain.log2(), 3),
            "bias_ev": round_to(s.exposure_bias, 2)}));
        img.scale(gain);
        return gain;
    }
    let (mut gain, info) =
        analyze::auto_exposure(thumb, p.target_key, p.exposure_strength, 3.2);
    gain *= s.exposure_bias.exp2();
    report.add("exposure", with(info, json!({"source": "automatic",
                                              "bias_ev": round_to(s.exposure_bias, 2)})));
    img.scale(gain);
    gain
}

/// Compress the scene's range on the low-frequency layer only.
pub fn apply_local_tone_map(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                            report: &mut Report) -> f32 {
    let (factor, info) = analyze::auto_tone_compression(thumb, p.comfortable_stops, 0.45);
    // Clarity is this same split between the broad light of the scene and its
    // fine structure, with the photographer setting how much of the structure
    // is kept.
    let detail_boost = p.detail_boost * (1.0 + 0.6 * s.clarity);
    let info = with(info, json!({"detail_boost": round_to(detail_boost, 3),
                                 "clarity": round_to(s.clarity, 2)}));
    if !s.on("tone_map") {
        report.skipped("tone mapping", "switched_off", info);
        return 1.0;
    }
    report.add("tone mapping", info);
    if factor >= 0.995 && (detail_boost - 1.0).abs() < 1e-3 {
        return factor;
    }

    let y = kit::luminance(img);
    let mut log_y = Plane::new(y.w, y.h);
    for i in 0..y.d.len() {
        log_y.d[i] = kit::fast_log2(y.d[i].max(1e-5));
    }
    let radius = ((img.w.max(img.h) as f32 / 45.0).round() as usize).max(8);
    let base = kit::guided_filter(&log_y, &log_y, radius, 0.15, 4);

    // anchor = 55th percentile of the base layer, sampled every 4th pixel
    let mut sample: Vec<f32> = Vec::with_capacity(base.d.len() / 16 + 1);
    for yy in (0..base.h).step_by(4) {
        for xx in (0..base.w).step_by(4) {
            sample.push(base.d[yy * base.w + xx]);
        }
    }
    let anchor = kit::percentile(&sample, 55.0);

    for (i, px) in img.px_mut().iter_mut().enumerate() {
        let detail = log_y.d[i] - base.d[i];
        let new_log = anchor + (base.d[i] - anchor) * factor + detail * detail_boost;
        let scale = kit::fast_exp2(new_log - log_y.d[i]);
        for v in px.iter_mut() {
            *v *= scale;
        }
    }
    factor
}

/// Scene-linear -> display-linear with a filmic shoulder.
pub fn filmic(img: &mut Image) {
    #[inline]
    fn curve(x: f32) -> f32 {
        let x = x.max(0.0);
        ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
    }
    for px in img.d.chunks_exact_mut(3) {
        let y = kit::luminance_px(px).max(EPS);
        let ty = curve(y);
        let w = kit::smoothstep(0.55, 1.0, ty);
        let hue_preserving = [px[0] * (ty / y), px[1] * (ty / y), px[2] * (ty / y)];
        let mut out = hue_preserving;
        if w > 0.0 {
            for c in 0..3 {
                out[c] = hue_preserving[c] * (1.0 - w) + curve(px[c]) * w;
            }
            // The per-channel curve is what bleaches a bright colour toward
            // white, and it is worth having.
            let (_, a0, b0) = kit::rgb_to_oklab_px(hue_preserving[0], hue_preserving[1],
                                                   hue_preserving[2]);
            let c0 = a0.hypot(b0);
            if c0 > 1e-4 {
                let (l, a, b) = kit::rgb_to_oklab_px(out[0], out[1], out[2]);
                let chroma = a.hypot(b);
                let (r, g, bl) = kit::oklab_to_rgb_px(l, a0 / c0 * chroma, b0 / c0 * chroma);
                out = [r, g, bl];
            }
        }
        for c in 0..3 {
            // Bounded only far outside anything a real colour reaches, so a
            // stray value cannot run away through the stages that follow.
            px[c] = out[c].clamp(-1.0, 2.0);
        }
    }
}

// -------------------------------------------------------------------------
// perceptual stages (Oklab)
// -------------------------------------------------------------------------

/// Steps in the contrast curve's per-render table.
const CURVE_STEPS: usize = 4096;

/// Smooth contrast curve anchored at the image's own midtone.
pub fn s_curve(x: f32, amount: f32, pivot: f32) -> f32 {
    if amount <= 1e-3 {
        return x;
    }
    let g = (0.5f32).ln() / pivot.clamp(0.1, 0.9).ln();
    let t = x.clamp(0.0, 1.0).powf(g);
    let shaped = kit::smoothstep(0.0, 1.0, t);
    ((1.0 - amount) * t + amount * shaped).powf(1.0 / g)
}

/// The most any one tone zone may move a lightness.
pub const ZONE_RANGE: f32 = 0.14;

/// How strongly each slider owns a given display lightness.
#[inline]
fn zone_weights(l: f32) -> (f32, f32, f32) {
    let shadow = 1.0 - kit::smoothstep(0.0, 0.5, l);
    let highlight = kit::smoothstep(0.5, 1.0, l);
    (shadow, 1.0 - shadow - highlight, highlight)
}

/// Shift one lightness by the three zone sliders.
#[inline]
pub fn apply_zones(l: f32, shadows: f32, midtones: f32, highlights: f32) -> f32 {
    if shadows == 0.0 && midtones == 0.0 && highlights == 0.0 {
        return l;
    }
    let (ws, wm, wh) = zone_weights(l);
    (l + ZONE_RANGE * (shadows * ws + midtones * wm + highlights * wh)).clamp(0.0, 1.0)
}

fn skin_protection(hue: f32, chroma: f32, skin: f32) -> f32 {
    let mut d = (hue - skin).abs();
    while d > std::f32::consts::PI {
        d = (2.0 * std::f32::consts::PI - d).abs();
    }
    (1.0 - kit::smoothstep(0.25, 0.60, d)) * (1.0 - kit::smoothstep(0.10, 0.20, chroma))
}

/// The part of a plane the estimators look at, at no more than 1024 pixels
/// on its long edge.
fn measured(p: &Plane, stats_rect: Option<Rect>) -> Plane {
    let view = stats_view_plane(p, stats_rect);
    let scale = (1024.0 / view.w.max(view.h) as f32).min(1.0);
    kit::resize_plane(&view, ((view.w as f32 * scale) as usize).max(1),
                      ((view.h as f32 * scale) as usize).max(1))
}

/// Why a noise reduction pass did not run.
fn denoise_skipped(on: bool, measured: f32) -> &'static str {
    if !on { "switched_off" } else if measured > 0.01 { "turned_down" } else { "clean_enough" }
}

/// Levels, contrast, vibrance, denoising and sharpening in Oklab.
pub fn apply_perceptual(img: &Image, s: &Settings, p: &Preset, noise: f32, sharpen_sigma: f32,
                        compression: f32, focus_sigma: f32, render_scale: f32,
                        report: &mut Report, stats_rect: Option<Rect>) -> Image {
    let n = img.w * img.h;
    let Lab { l: mut lightness, mut a, mut b } = Lab::from_rgb(img);
    for i in 0..n {
        // A colour far enough outside sRGB can have positive luminance and
        // still land below zero lightness here.
        if !(lightness.d[i] > 0.0) {
            lightness.d[i] = 0.0;
            a.d[i] = 0.0;
            b.d[i] = 0.0;
        }
    }

    // --- black/white point ------------------------------------------------
    let (black, white, info) =
        analyze::auto_levels(&measured(&lightness, stats_rect), p.levels_strength, 0.15);
    let levels_applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut light_new = Plane::new(img.w, img.h);
    if s.on("levels") && levels_applied {
        report.add("levels", info);
        let span = (white - black).max(1e-6);
        for i in 0..n {
            light_new.d[i] = ((lightness.d[i] - black) / span).clamp(0.0, 1.0);
        }
    } else {
        report.skipped("levels", if !s.on("levels") { "switched_off" } else { "not_needed" }, info);
        light_new.d.copy_from_slice(&lightness.d);
    }

    // --- contrast ---------------------------------------------------------
    let (amount, pivot, info) = analyze::auto_contrast(&measured(&light_new, stats_rect), 0.20,
                                                       p.contrast_amount, compression);
    if s.on("contrast") {
        report.add("contrast", info);
        // One curve for the whole frame, so it is tabulated once rather than
        // evaluated with two `powf` per pixel.
        let table: Vec<f32> = (0..=CURVE_STEPS + 1)
            .map(|i| s_curve((i as f32 / CURVE_STEPS as f32).min(1.0), amount, pivot))
            .collect();
        for v in light_new.d.iter_mut() {
            let f = v.clamp(0.0, 1.0) * CURVE_STEPS as f32;
            let j = f as usize;
            *v = table[j] + (table[j + 1] - table[j]) * (f - j as f32);
        }
    } else {
        report.skipped("contrast", "switched_off", info);
    }

    // --- tone zones -------------------------------------------------------
    // The user's own shadows/midtones/highlights, on top of whatever the
    // automatic curve decided.
    if s.shadows != 0.0 || s.midtones != 0.0 || s.highlights != 0.0 {
        for i in 0..n {
            light_new.d[i] = apply_zones(light_new.d[i], s.shadows, s.midtones, s.highlights);
        }
        report.add("tone zones", json!({"applied": true,
            "shadows": round_to(s.shadows, 3), "midtones": round_to(s.midtones, 3),
            "highlights": round_to(s.highlights, 3),
            "range": round_to(ZONE_RANGE, 3)}));
    } else {
        report.skipped("tone zones", "not_needed", json!({}));
    }

    // Lifting shadows leaves their colour behind; track the lightness change so
    // saturation stays proportional instead of going grey.
    for i in 0..n {
        let ratio = (light_new.d[i].max(0.0) + 1e-3) / (lightness.d[i].max(0.0) + 1e-3);
        let comp = kit::fast_exp2(0.35 * kit::fast_log2(ratio)).clamp(0.6, 1.8);
        a.d[i] *= comp;
        b.d[i] *= comp;
    }
    drop(lightness);

    // --- colour noise -----------------------------------------------------
    // The photographer's amount works from the measured one rather than
    // replacing it, so "a little more" means a little more for this frame.
    let auto_chroma = if p.denoise { ((noise - 0.18) / 0.5).clamp(0.0, 1.0) * 0.85 } else { 0.0 };
    let chroma_blend = with_user(auto_chroma, s.denoise, 0.95);
    if s.on("denoise_chroma") && chroma_blend > 0.01 {
        let radius = ((img.w.max(img.h) as f32 / 350.0).round() as usize).max(3);
        let blend = chroma_blend;
        let fa = kit::guided_filter(&light_new, &a, radius, 4e-4, 2);
        let fb = kit::guided_filter(&light_new, &b, radius, 4e-4, 2);
        for i in 0..n {
            a.d[i] = a.d[i] * (1.0 - blend) + fa.d[i] * blend;
            b.d[i] = b.d[i] * (1.0 - blend) + fb.d[i] * blend;
        }
        report.add("colour noise",
            json!({"applied": true, "radius": radius, "blend": round_to(blend, 3),
                   "measured": round_to(auto_chroma, 3), "user": round_to(s.denoise, 2)}));
    } else {
        report.skipped("colour noise", denoise_skipped(s.on("denoise_chroma"), auto_chroma),
                       json!({"noise": round_to(noise, 3)}));
    }

    // --- luminance noise --------------------------------------------------
    // Allowed further than the automation goes on its own (0.85 against 0.55),
    // because the photographer can see what it costs in texture and the
    // automation cannot.
    let auto_luma = if p.denoise { ((noise - 0.30) / 0.6).clamp(0.0, 1.0) * 0.55 } else { 0.0 };
    let luma_blend = with_user(auto_luma, s.denoise, 0.85);
    if s.on("denoise_luma") && luma_blend > 0.01 {
        let radius = ((img.w.max(img.h) as f32 / 900.0).round() as usize).max(2);
        let blend = luma_blend;
        let fl = kit::guided_filter(&light_new, &light_new, radius, 2.5e-4, 2);
        for i in 0..n {
            light_new.d[i] = light_new.d[i] * (1.0 - blend) + fl.d[i] * blend;
        }
        report.add("luminance noise",
            json!({"applied": true, "radius": radius, "blend": round_to(blend, 3),
                   "measured": round_to(auto_luma, 3), "user": round_to(s.denoise, 2)}));
    } else {
        report.skipped("luminance noise", denoise_skipped(s.on("denoise_luma"), auto_luma),
                       json!({"noise": round_to(noise, 3)}));
    }

    // --- vibrance ---------------------------------------------------------
    let mut chroma = Plane::new(img.w, img.h);
    for i in 0..n {
        chroma.d[i] = a.d[i].hypot(b.d[i]);
    }
    let (mut boost, info) = analyze::auto_vibrance(
        &measured(&chroma, stats_rect),
        &measured(&light_new, stats_rect),
        p.target_chroma,
        p.max_boost,
    );
    // Chroma noise rides up with the colour.
    let restraint = (1.0 - 0.6 * noise).clamp(0.3, 1.0);
    boost = 1.0 + (boost - 1.0) * restraint;
    boost = (boost * (1.0 + 0.55 * s.vibrance)).clamp(0.45, 2.4);
    let info = with(info, json!({"noise_restraint": round_to(restraint, 3),
                                 "user": round_to(s.vibrance, 3),
                                 "final_boost": round_to(boost, 3)}));
    if !s.on("vibrance") {
        report.skipped("vibrance", "switched_off", info);
        boost = 1.0;
    } else {
        report.add("vibrance", info);
    }
    if (boost - 1.0).abs() > 0.001 {
        let skin = skin_hue();
        for i in 0..n {
            let c = chroma.d[i];
            let hue = kit::fast_atan2(b.d[i], a.d[i]);
            let mut weight = 1.0 - kit::smoothstep(0.10, 0.30, c);
            weight *= 1.0 - 0.55 * skin_protection(hue, c, skin);
            let gain = 1.0 + (boost - 1.0) * weight;
            a.d[i] *= gain;
            b.d[i] *= gain;
        }
    }
    drop(chroma);

    // --- focus recovery ---------------------------------------------------
    // Before sharpening, because the two are not the same thing and the order
    // matters.
    if s.refocus > 0.0 && s.on("refocus") && focus_sigma > 0.0 {
        let iterations = recover_focus(&mut light_new, focus_sigma, s.refocus, noise);
        report.add("focus recovery", json!({"applied": true,
            "sigma_px": round_to(focus_sigma, 2), "amount": round_to(s.refocus, 2),
            "iterations": iterations}));
    } else {
        let reason = if !s.on("refocus") { "switched_off" }
                     else if focus_sigma <= 0.0 { "sharp_enough" }
                     else { "not_asked" };
        report.skipped("focus recovery", reason, json!({}));
    }

    // --- sharpening -------------------------------------------------------
    let amount = p.sharpen * (1.0 - 0.75 * noise) * render_scale.clamp(0.25, 1.0);
    if s.on("sharpen") && amount > 0.02 {
        let blurred = kit::gaussian_blur(&light_new, sharpen_sigma);
        let floor = 0.004 + 0.02 * noise;
        for i in 0..n {
            let mut detail = light_new.d[i] - blurred.d[i];
            detail *= kit::smoothstep(floor, floor * 3.0, detail.abs());
            light_new.d[i] = (light_new.d[i] + (detail * amount).clamp(-0.07, 0.07)).clamp(0.0, 1.0);
        }
        report.add("sharpening", json!({"applied": true, "amount": round_to(amount, 3),
            "radius_px": round_to(sharpen_sigma, 2), "noise_floor": round_to(floor, 4)}));
    } else {
        let reason = if !s.on("sharpen") { "switched_off" } else { "too_noisy" };
        report.skipped("sharpening", reason, json!({"noise": round_to(noise, 3)}));
    }

    kit::to_gamut(&light_new, &a, &b)
}

/// A measured amount, moved by the photographer's -1..1.
fn with_user(measured: f32, user: f32, ceiling: f32) -> f32 {
    if user < 0.0 {
        measured * (1.0 + user)
    } else {
        measured + (ceiling - measured).max(0.0) * 0.6 * user
    }
}

/// Undo a measured blur, as far as a blur can be undone.
pub fn recover_focus(l: &mut Plane, sigma: f32, amount: f32, noise: f32) -> usize {
    // A wide blur needs more rounds than a narrow one, and a noisy frame can
    // afford fewer.
    let rounds = ((2.0 + 3.0 * sigma * amount) * (1.0 - 0.5 * noise)).round()
        .clamp(1.0, 12.0) as usize;
    // Held below one on purpose.
    let step = 0.55 * amount;
    // How far any one pixel may be moved in total.
    let ceiling = (0.10 + 0.14 * amount).min(0.24);
    let guard = 0.004 + 0.02 * noise;

    let original = l.clone();
    for _ in 0..rounds {
        let blurred = kit::gaussian_blur(l, sigma);
        for i in 0..l.d.len() {
            let residual = original.d[i] - blurred.d[i];
            // Where the frame disagrees with its own blur by less than its
            // noise, the disagreement is the noise. Leave it alone.
            let live = residual * kit::smoothstep(guard, guard * 3.0, residual.abs());
            let moved = (l.d[i] + step * live - original.d[i]).clamp(-ceiling, ceiling);
            l.d[i] = (original.d[i] + moved).clamp(0.0, 1.0);
        }
    }
    rounds
}

// -------------------------------------------------------------------------
// entry point
// -------------------------------------------------------------------------

/// Where each stage sits along the run, for the progress bar.  Stage *codes*,
/// not sentences: the interface names them in its own language.
pub const STAGE_MARKS: [(f32, &str); 7] = [
    (0.00, "white_balance"),
    (0.07, "exposure"),
    (0.11, "noise"),
    (0.15, "tone_map"),
    (0.27, "tone_curve"),
    (0.40, "perceptual"),
    (1.00, "done"),
];

pub struct ProcessArgs<'a> {
    pub iso: f32,
    pub sharpen_sigma: f32,
    pub noise_prior: f32,
    pub stats_rect: Option<Rect>,
    /// This render's long edge over the full frame's: 1 for an export.
    pub render_scale: f32,
    pub preset: Preset,
    /// The noise floor measured once at full resolution.
    pub native_noise_floor: f32,
    /// The blur `analyze::focus_width` measured, in pixels of the *rendered*
    /// frame.
    pub focus_sigma: f32,
    pub progress: Option<&'a mut dyn FnMut(f32, &str)>,
}

/// Linear scene-referred RGB -> display-referred linear sRGB in [0,1].
pub fn process(src: &Image, settings: &Settings, args: ProcessArgs, report: &mut Report) -> Image {
    let p = args.preset;
    let mut img = src.clone();
    // Healing before anything is measured; see `local`.
    crate::local::heal(&mut img, &settings.spots, args.stats_rect);
    report.add("healing", json!({"spots": settings.spots.len()}));
    let mut progress = args.progress;
    let mut mark = 0usize;
    let step = |progress: &mut Option<&mut dyn FnMut(f32, &str)>, mark: &mut usize| {
        if let Some(cb) = progress {
            let (f, code) = STAGE_MARKS[*mark];
            cb(f, code);
        }
        *mark += 1;
    };

    let measure = |img: &Image, rect: Option<Rect>| kit::thumbnail(&stats_view_img(img, rect), 1024);

    step(&mut progress, &mut mark);
    // One thumbnail for every scene-linear measurement, carried through the
    // same white balance and exposure as the frame (see apply_white_balance).
    let mut thumb = measure(&img, args.stats_rect);
    apply_white_balance(&mut img, &mut thumb, settings, &p, report);
    step(&mut progress, &mut mark);
    let exposure_gain = apply_exposure(&mut img, &thumb, settings, &p, report);
    thumb.scale(exposure_gain);
    step(&mut progress, &mut mark);

    // Noise must be measured at native resolution.
    let (noise, info) = analyze::noise_from_floor(
        args.native_noise_floor * exposure_gain.max(1e-6).sqrt(), args.iso, args.noise_prior);
    report.add("noise", info);

    step(&mut progress, &mut mark);
    let compression = apply_local_tone_map(&mut img, &thumb, settings, &p, report);
    // Local filters on the light as it will be shown, before the shoulder
    // rolls it off; see `local` for why not earlier.
    crate::local::apply_filters(&mut img, &settings.filters, args.stats_rect);
    report.add("filters", json!({"count": settings.filters.len()}));
    step(&mut progress, &mut mark);
    filmic(&mut img);
    step(&mut progress, &mut mark);
    let out = apply_perceptual(&img, settings, &p, noise, args.sharpen_sigma, compression,
                               args.focus_sigma, args.render_scale, report, args.stats_rect);
    step(&mut progress, &mut mark);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The S-curve is pivoted on the image's own midtone, so it must hold the
    /// pivot in place while it steepens either side of it.
    #[test]
    fn s_curve_matches_reference() {
        for (x, want) in [(0.1f32, 0.071561f32), (0.25, 0.218064), (0.5, 0.509989),
                          (0.75, 0.792776), (0.9, 0.929976)] {
            let got = s_curve(x, 0.4, 0.45);
            assert!((got - want).abs() < 1e-4, "x={} got {} want {}", x, got, want);
        }
        assert_eq!(s_curve(0.3, 0.0, 0.5), 0.3);
    }

    /// Grey goes exactly where the gains send it, because the gains are what
    /// the eyedropper and the automatic balance decided.
    #[test]
    fn adaptation_agrees_with_the_gains_on_grey() {
        let gains = [0.62f32, 1.0, 1.55];
        let mut img = Image::new(2, 1);
        img.d.copy_from_slice(&[0.3 / 0.62 * 0.5, 0.3 * 0.5, 0.3 / 1.55 * 0.5, 0.6, 0.1, 0.05]);
        let mut by_gains = img.clone();
        by_gains.scale_channels(gains);
        adapt(&mut img, gains);
        for c in 0..3 {
            assert!((img.d[c] - by_gains.d[c]).abs() < 1e-4, "grey went elsewhere: {:?}", &img.d[..3]);
        }
        let apart = (0..3).map(|c| (img.d[3 + c] - by_gains.d[3 + c]).abs()).fold(0.0f32, f32::max);
        assert!(apart > 0.005, "a saturated colour was treated like three volume knobs");
        let mut same = by_gains.clone();
        adapt(&mut same, [1.0, 1.0, 1.0]);
        assert_eq!(same.d, by_gains.d, "no correction must mean no change");
    }

    /// The shoulder bleaches bright colours toward white without turning them.
    #[test]
    fn the_shoulder_keeps_hue() {
        let hue_and_chroma = |px: &[f32]| {
            let (_, a, b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
            (b.atan2(a).to_degrees(), a.hypot(b))
        };
        for c in [[1.0f32, 0.45, 0.08], [0.25, 0.5, 1.0], [1.0, 0.62, 0.45], [0.35, 0.8, 0.15]] {
            let mut img = Image::new(1, 1);
            img.d.copy_from_slice(&[c[0] * 4.0, c[1] * 4.0, c[2] * 4.0]);
            let (h0, c0) = hue_and_chroma(&img.d);
            filmic(&mut img);
            let (h1, c1) = hue_and_chroma(&img.d);
            let mut shift = (h1 - h0).abs();
            if shift > 180.0 { shift = 360.0 - shift; }
            assert!(shift < 1.0, "{c:?} turned by {shift} degrees");
            assert!(c1 < c0, "{c:?} was not bleached toward white: {c0} -> {c1}");
        }
    }

    /// The whole point of capping ZONE_RANGE.
    #[test]
    fn tone_zones_stay_monotonic() {
        let steps = [-1.0f32, -0.5, 0.0, 0.5, 1.0];
        for &sh in &steps {
            for &mid in &steps {
                for &hi in &steps {
                    let mut previous = f32::NEG_INFINITY;
                    for i in 0..=1000 {
                        let l = i as f32 / 1000.0;
                        let out = apply_zones(l, sh, mid, hi);
                        assert!(out >= previous - 1e-6,
                            "curve folded back at l={} for ({}, {}, {}): {} after {}",
                            l, sh, mid, hi, out, previous);
                        previous = out;
                    }
                }
            }
        }
    }

    /// The weights are a partition of unity, so three equal sliders are a plain
    /// uniform lift. If this drifts, the zones have started double counting.
    #[test]
    fn equal_zones_are_a_uniform_lift() {
        for &amount in &[-1.0f32, -0.4, 0.4, 1.0] {
            for i in 1..10 {
                let l = i as f32 / 10.0;
                let expected = (l + ZONE_RANGE * amount).clamp(0.0, 1.0);
                let got = apply_zones(l, amount, amount, amount);
                assert!((got - expected).abs() < 1e-5,
                    "l={} amount={}: got {} want {}", l, amount, got, expected);
            }
        }
    }

    /// Zero has to mean *exactly* untouched, not nearly: every frame is
    /// developed through this path whether or not the sliders were moved.
    #[test]
    fn zero_zones_are_the_identity() {
        for i in 0..=100 {
            let l = i as f32 / 100.0;
            assert_eq!(apply_zones(l, 0.0, 0.0, 0.0), l);
        }
    }

    /// Each slider has to own its end of the range and leave the other alone,
    /// which is what "isolate the parts of the picture" means in practice.
    #[test]
    fn zones_isolate_their_own_tones() {
        let (dark, mid, bright) = (0.08f32, 0.5, 0.92);
        let moved = |a: f32, b: f32| (a - b).abs();

        let sh = |l: f32| apply_zones(l, 1.0, 0.0, 0.0);
        assert!(moved(sh(dark), dark) > 0.10, "shadows slider did not lift the shadows");
        assert!(moved(sh(bright), bright) < 0.01, "shadows slider disturbed the highlights");

        let hi = |l: f32| apply_zones(l, 0.0, 0.0, -1.0);
        assert!(moved(hi(bright), bright) > 0.10, "highlights slider did not pull the highlights");
        assert!(moved(hi(dark), dark) < 0.01, "highlights slider disturbed the shadows");

        let md = |l: f32| apply_zones(l, 0.0, 1.0, 0.0);
        assert!(moved(md(mid), mid) > 0.10, "midtones slider did not move the midtones");
        assert!(moved(md(dark), dark) < 0.02 && moved(md(bright), bright) < 0.02,
                "midtones slider reached the ends of the range");
    }

    /// A textured frame, a few stops of light across it and fine structure
    /// on top: what clarity has to find and scale.
    fn textured() -> Image {
        let (w, h) = (256usize, 128usize);
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let broad = 0.02 * 2f32.powf(7.0 * x as f32 / w as f32);
                let fine = if (x / 3 + y / 3) % 2 == 0 { 1.15 } else { 0.87 };
                for c in 0..3 {
                    img.d[(y * w + x) * 3 + c] = broad * fine;
                }
            }
        }
        img
    }

    /// How much fine structure a frame carries: the spread of each pixel
    /// against the average of its row neighbours, in stops.
    fn fine_structure(img: &Image) -> f32 {
        let y = kit::luminance(img);
        let mut sum = 0.0f32;
        let mut n = 0.0f32;
        for row in 0..y.h {
            for x in 3..y.w - 3 {
                let i = row * y.w + x;
                let around = (y.d[i - 3] + y.d[i + 3]) * 0.5;
                sum += (y.d[i].max(1e-6) / around.max(1e-6)).log2().abs();
                n += 1.0;
            }
        }
        sum / n
    }

    #[test]
    fn clarity_scales_the_fine_structure() {
        let src = textured();
        let run = |clarity: f32| {
            let mut s = Settings::default();
            s.clarity = clarity;
            let mut img = src.clone();
            let mut report = Report::new();
            apply_local_tone_map(&mut img, &src.clone(), &s, &Preset::default(), &mut report);
            fine_structure(&img)
        };
        let (soft, plain, crisp) = (run(-1.0), run(0.0), run(1.0));
        assert!(soft < plain * 0.8 && crisp > plain * 1.2,
                "clarity did not move the fine structure: {soft} {plain} {crisp}");
    }

    /// Minus one is off, even on a frame the automation would have cleaned
    /// hard; plus one reaches a frame it would have left alone.
    #[test]
    fn noise_reduction_follows_the_photographer() {
        let src = textured();
        let run = |denoise: f32, noise: f32| -> Value {
            let mut s = Settings::default();
            s.denoise = denoise;
            let mut report = Report::new();
            apply_perceptual(&src, &s, &Preset::default(), noise, 1.0, 1.0, 0.0, 1.0, &mut report, None);
            report.to_json()
        };
        let applied = |r: &Value, stage: &str| r[stage]["applied"].as_bool().unwrap_or(false);
        let noisy_off = run(-1.0, 0.9);
        assert!(!applied(&noisy_off, "colour noise") && !applied(&noisy_off, "luminance noise"),
                "-1 did not turn noise reduction off: {noisy_off}");
        let clean_on = run(1.0, 0.05);
        assert!(applied(&clean_on, "colour noise") && applied(&clean_on, "luminance noise"),
                "+1 did not reach a clean frame: {clean_on}");
        let untouched = run(0.0, 0.05);
        assert!(!applied(&untouched, "colour noise"), "a clean frame was denoised at zero");
    }

    #[test]
    fn settings_parse_from_the_wire() {
        let v = serde_json::json!({
            "framing": {"angle": 2.5, "auto_fit": false, "crop": [0.1, 0.1, 0.5, 0.5]},
            "vibrance": 0.4, "shadows": 0.6, "midtones": -0.2, "highlights": 4.0,
            "clarity": 0.5, "denoise": -3.0,
            "preset": "punchy", "enabled": {"sharpen": false}});
        let s = Settings::from_json(&v);
        assert_eq!(s.shadows, 0.6);
        assert_eq!(s.midtones, -0.2);
        assert_eq!(s.highlights, 1.0, "out-of-range slider was not clamped");
        assert_eq!(s.clarity, 0.5);
        assert_eq!(s.denoise, -1.0, "out-of-range noise reduction was not clamped");
        assert_eq!(s.framing.angle, 2.5);
        assert_eq!(s.framing.auto_fit, false);
        assert_eq!(s.framing.crop, Some((0.1, 0.1, 0.5, 0.5)));
        assert!(!s.on("sharpen"));
        assert!(s.on("contrast"));
        assert_eq!(s.preset_obj().name, "punchy");
    }
}

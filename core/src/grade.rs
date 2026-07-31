//! The grading pipeline: linear raw in, display-ready sRGB out.

use crate::analyze::{self, Report};
use crate::geometry::{stats_view_img, stats_view_plane, Framing, Rect};
use crate::ops::{self, Image, Plane};
use serde_json::{json, Map, Value};
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
pub const TOGGLES: [(&str, &str, &str); 12] = [
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
    pub preset: String,
    pub render_scale: f32,
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
            preset: "natural".into(),
            render_scale: 1.0,
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
        let rect = |k: &str| -> Option<Rect> {
            let a = obj.get(k)?.as_array()?;
            if a.len() < 4 {
                return None;
            }
            let g = |i: usize| a[i].as_f64().unwrap_or(0.0) as f32;
            Some((g(0), g(1), g(2), g(3)))
        };
        s.exposure_rect = rect("exposure_rect");
        s.wb_rect = rect("wb_rect");
        s.exposure_bias = f("exposure_bias").unwrap_or(0.0);
        s.temperature = f("temperature").unwrap_or(0.0);
        s.tint = f("tint").unwrap_or(0.0);
        s.vibrance = f("vibrance").unwrap_or(0.0);
        s.shadows = f("shadows").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.midtones = f("midtones").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.highlights = f("highlights").unwrap_or(0.0).clamp(-1.0, 1.0);
        if let Some(p) = obj.get("preset").and_then(|x| x.as_str()) {
            s.preset = p.to_string();
        }
        if let Some(fr) = obj.get("framing").and_then(|x| x.as_object()) {
            let g = |k: &str| fr.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
            s.framing.angle = g("angle");
            s.framing.perspective_v = g("perspective_v");
            s.framing.perspective_h = g("perspective_h");
            s.framing.auto_fit = fr.get("auto_fit").and_then(|x| x.as_bool()).unwrap_or(true);
            s.framing.crop = fr.get("crop").and_then(|c| {
                let a = c.as_array()?;
                if a.len() < 4 {
                    return None;
                }
                let g = |i: usize| a[i].as_f64().unwrap_or(0.0) as f32;
                Some((g(0), g(1), g(2), g(3)))
            });
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

    /// Cache key for a settings-dependent render.
    pub fn key(&self) -> String {
        let mut flags: Vec<String> =
            self.enabled.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        flags.sort();
        format!(
            "{}|{:?}|{:?}|{:.4}|{:.4}|{:.4}|{:.4}|{:.4}|{:.4}|{:.4}|{}|{}",
            self.framing.key(),
            self.exposure_rect,
            self.wb_rect,
            self.exposure_bias,
            self.temperature,
            self.tint,
            self.vibrance,
            self.shadows,
            self.midtones,
            self.highlights,
            self.preset,
            flags.join(",")
        )
    }
}

fn round_to(v: f32, places: i32) -> f64 {
    let f = 10f64.powi(places);
    ((v as f64) * f).round() / f
}

// Hue of average skin in Oklab, so the saturation stage can leave faces alone.
fn skin_hue() -> f32 {
    let r = ops::srgb_decode_scalar(0.76);
    let g = ops::srgb_decode_scalar(0.57);
    let b = ops::srgb_decode_scalar(0.46);
    let (_, a, bb) = ops::rgb_to_oklab_px(r, g, b);
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
    let mut keep = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for yy in y0..y1 {
        for xx in x0..x1 {
            let i = (yy * img.w + xx) * 3;
            let px = [img.d[i], img.d[i + 1], img.d[i + 2]];
            all.push(px);
            if px[0].max(px[1]).max(px[2]) < 0.99 {
                keep.push(px);
            }
        }
    }
    if all.is_empty() {
        return None;
    }
    Some(if keep.len() >= 16 { keep } else { all })
}

// -------------------------------------------------------------------------
// scene-linear stages
// -------------------------------------------------------------------------

pub fn apply_white_balance(img: &mut Image, thumb: &Image, s: &Settings, report: &mut Report) {
    if !s.on("white_balance") {
        report.add("white balance", json!({"applied": false, "reason": "switched_off"}));
        apply_temp_tint(img, s, report);
        return;
    }
    if let Some(rect) = s.wb_rect {
        if let Some(patch) = sample_rect(img, rect) {
            if patch.len() >= 4 {
                let n = patch.len() as f32;
                let mut mean = [0f32; 3];
                for px in &patch {
                    for c in 0..3 {
                        mean[c] += px[c];
                    }
                }
                for c in 0..3 {
                    mean[c] = (mean[c] / n).max(EPS);
                }
                let avg = (mean[0] + mean[1] + mean[2]) / 3.0;
                let mut gains = [avg / mean[0], avg / mean[1], avg / mean[2]];
                let lw = [0.2126f32, 0.7152, 0.0722];
                let norm: f32 = (0..3).map(|c| gains[c] * lw[c]).sum();
                for c in 0..3 {
                    gains[c] /= norm;
                }
                report.add("white balance", json!({"applied": true, "source": "selection",
                    "gains": [round_to(gains[0],4), round_to(gains[1],4), round_to(gains[2],4)],
                    "patch_px": patch.len()}));
                img.scale_channels(gains);
                apply_temp_tint(img, s, report);
                return;
            }
        }
    }
    let (gains, mut info) = analyze::auto_white_balance(thumb, s.preset_obj().wb_strength, WB_LIMIT_EV);
    if let Some(o) = info.as_object_mut() {
        o.insert("source".into(), json!("automatic"));
    }
    let applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
    report.add("white balance", info);
    if applied {
        img.scale_channels(gains);
    }
    apply_temp_tint(img, s, report);
}

fn apply_temp_tint(img: &mut Image, s: &Settings, report: &mut Report) {
    if s.temperature.abs() < 1e-3 && s.tint.abs() < 1e-3 {
        return;
    }
    let mut log_gain = [
        0.35 * s.temperature - 0.125 * s.tint,
        0.25 * s.tint,
        -0.35 * s.temperature - 0.125 * s.tint,
    ];
    let lw = [0.2126f32, 0.7152, 0.0722];
    let weighted: f32 = (0..3).map(|c| log_gain[c] * lw[c]).sum();
    let mut gains = [0f32; 3];
    for c in 0..3 {
        log_gain[c] -= weighted;
        gains[c] = log_gain[c].exp2();
    }
    report.add("temp/tint", json!({"applied": true, "temperature": round_to(s.temperature,3),
        "tint": round_to(s.tint,3),
        "gains": [round_to(gains[0],4), round_to(gains[1],4), round_to(gains[2],4)]}));
    img.scale_channels(gains);
}

pub fn apply_exposure(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                      report: &mut Report) -> f32 {
    if !s.on("exposure") {
        report.add("exposure", json!({"applied": false, "reason": "switched_off"}));
        let gain = s.exposure_bias.exp2();
        img.scale(gain);
        return gain;
    }
    if let Some(rect) = s.exposure_rect {
        if let Some(patch) = sample_rect(img, rect) {
            if !patch.is_empty() {
                // Geometric mean: a few bright specks in the selection should
                // not drag the whole frame down.
                let log_sum: f64 = patch
                    .iter()
                    .map(|px| ops::luminance_px(px).max(EPS).log2() as f64)
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
        }
    }
    let (mut gain, mut info) =
        analyze::auto_exposure(thumb, p.target_key, p.exposure_strength, 3.2);
    gain *= s.exposure_bias.exp2();
    if let Some(o) = info.as_object_mut() {
        o.insert("source".into(), json!("automatic"));
        o.insert("bias_ev".into(), json!(round_to(s.exposure_bias, 2)));
    }
    report.add("exposure", info);
    img.scale(gain);
    gain
}

/// Compress the scene's range on the low-frequency layer only.
pub fn apply_local_tone_map(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                            report: &mut Report) -> f32 {
    let (factor, mut info) = analyze::auto_tone_compression(thumb, p.comfortable_stops, 0.45);
    if let Some(o) = info.as_object_mut() {
        o.insert("detail_boost".into(), json!(round_to(p.detail_boost, 3)));
    }
    if !s.on("tone_map") {
        let mut off = info.clone();
        if let Some(o) = off.as_object_mut() {
            o.insert("applied".into(), json!(false));
            o.insert("reason".into(), json!("switched_off"));
        }
        report.add("tone mapping", off);
        return 1.0;
    }
    report.add("tone mapping", info);
    if factor >= 0.995 && p.detail_boost <= 1.0 {
        return factor;
    }

    let y = ops::luminance(img);
    let mut log_y = Plane::new(y.w, y.h);
    for i in 0..y.d.len() {
        log_y.d[i] = y.d[i].max(1e-5).log2();
    }
    let radius = ((img.w.max(img.h) as f32 / 45.0).round() as usize).max(8);
    let base = ops::guided_filter(&log_y, &log_y, radius, 0.15, 4);

    // anchor = 55th percentile of the base layer, sampled every 4th pixel
    let mut sample: Vec<f32> = Vec::with_capacity(base.d.len() / 16 + 1);
    for yy in (0..base.h).step_by(4) {
        for xx in (0..base.w).step_by(4) {
            sample.push(base.d[yy * base.w + xx]);
        }
    }
    let anchor = ops::percentile(&sample, 55.0);

    for i in 0..y.d.len() {
        let detail = log_y.d[i] - base.d[i];
        let new_log = anchor + (base.d[i] - anchor) * factor + detail * p.detail_boost;
        let scale = (new_log - log_y.d[i]).exp2();
        for c in 0..3 {
            img.d[i * 3 + c] *= scale;
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
        let y = ops::luminance_px(px).max(EPS);
        let ty = curve(y);
        let w = ops::smoothstep(0.55, 1.0, ty);
        for c in 0..3 {
            let hue_preserving = px[c] * (ty / y);
            let per_channel = curve(px[c]);
            px[c] = (hue_preserving * (1.0 - w) + per_channel * w).clamp(0.0, 1.0);
        }
    }
}

// -------------------------------------------------------------------------
// perceptual stages (Oklab)
// -------------------------------------------------------------------------

/// Smooth contrast curve anchored at the image's own midtone.
pub fn s_curve(x: f32, amount: f32, pivot: f32) -> f32 {
    if amount <= 1e-3 {
        return x;
    }
    let g = (0.5f32).ln() / pivot.clamp(0.1, 0.9).ln();
    let t = x.clamp(0.0, 1.0).powf(g);
    let shaped = t * t * (3.0 - 2.0 * t);
    ((1.0 - amount) * t + amount * shaped).powf(1.0 / g)
}

/// The most any one tone zone may move a lightness.
pub const ZONE_RANGE: f32 = 0.14;

/// How strongly each slider owns a given display lightness.
#[inline]
fn zone_weights(l: f32) -> (f32, f32, f32) {
    let shadow = 1.0 - ops::smoothstep(0.0, 0.5, l);
    let highlight = ops::smoothstep(0.5, 1.0, l);
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
    (1.0 - ops::smoothstep(0.25, 0.60, d)) * (1.0 - ops::smoothstep(0.10, 0.20, chroma))
}

fn thumb_size(w: usize, h: usize) -> (usize, usize) {
    let scale = (1024.0 / w.max(h) as f32).min(1.0);
    (((w as f32 * scale) as usize).max(1), ((h as f32 * scale) as usize).max(1))
}

fn centre_crop(img: &Image, size: usize) -> Image {
    let w = size.min(img.w);
    let h = size.min(img.h);
    let x = (img.w - w) / 2;
    let y = (img.h - h) / 2;
    img.crop(x, y, w, h)
}

/// Levels, contrast, vibrance, denoising and sharpening in Oklab.
pub fn apply_perceptual(img: &Image, s: &Settings, p: &Preset, noise: f32, sharpen_sigma: f32,
                        compression: f32, report: &mut Report, stats_rect: Option<Rect>) -> Image {
    let n = img.w * img.h;
    let mut lightness = Plane::new(img.w, img.h);
    let mut a = Plane::new(img.w, img.h);
    let mut b = Plane::new(img.w, img.h);
    for i in 0..n {
        let (l, aa, bb) = ops::rgb_to_oklab_px(img.d[i * 3], img.d[i * 3 + 1], img.d[i * 3 + 2]);
        lightness.d[i] = l;
        a.d[i] = aa;
        b.d[i] = bb;
    }

    // --- black/white point ------------------------------------------------
    let measured = stats_view_plane(&lightness, stats_rect);
    let (tw, th) = thumb_size(measured.w, measured.h);
    let small = ops::resize_plane(&measured, tw, th);
    let (black, white, info) = analyze::auto_levels(&small, p.levels_strength, 0.15);
    let levels_applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut light_new = Plane::new(img.w, img.h);
    if s.on("levels") && levels_applied {
        report.add("levels", info);
        let span = (white - black).max(1e-6);
        for i in 0..n {
            light_new.d[i] = ((lightness.d[i] - black) / span).clamp(0.0, 1.0);
        }
    } else {
        let mut off = info.clone();
        if let Some(o) = off.as_object_mut() {
            o.insert("applied".into(), json!(false));
            o.insert(
                "reason".into(),
                json!(if !s.on("levels") { "switched_off" } else { "not_needed" }),
            );
        }
        report.add("levels", off);
        light_new.d.copy_from_slice(&lightness.d);
    }

    // --- contrast ---------------------------------------------------------
    let measured = stats_view_plane(&light_new, stats_rect);
    let (tw, th) = thumb_size(measured.w, measured.h);
    let small = ops::resize_plane(&measured, tw, th);
    let (amount, pivot, info) =
        analyze::auto_contrast(&small, 0.20, p.contrast_amount, compression);
    if s.on("contrast") {
        report.add("contrast", info);
        for i in 0..n {
            light_new.d[i] = s_curve(light_new.d[i], amount, pivot);
        }
    } else {
        let mut off = info.clone();
        if let Some(o) = off.as_object_mut() {
            o.insert("applied".into(), json!(false));
            o.insert("reason".into(), json!("switched_off"));
        }
        report.add("contrast", off);
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
        report.add("tone zones", json!({"applied": false, "reason": "not_needed"}));
    }

    // Lifting shadows leaves their colour behind; track the lightness change so
    // saturation stays proportional instead of going grey.
    for i in 0..n {
        let comp = (((light_new.d[i] + 1e-3) / (lightness.d[i] + 1e-3)).powf(0.35)).clamp(0.6, 1.8);
        a.d[i] *= comp;
        b.d[i] *= comp;
    }
    drop(lightness);

    // --- colour noise -----------------------------------------------------
    if s.on("denoise_chroma") && p.denoise && noise > 0.18 {
        let radius = ((img.w.max(img.h) as f32 / 350.0).round() as usize).max(3);
        let blend = ((noise - 0.18) / 0.5).clamp(0.0, 1.0) * 0.85;
        let fa = ops::guided_filter(&light_new, &a, radius, 4e-4, 2);
        let fb = ops::guided_filter(&light_new, &b, radius, 4e-4, 2);
        for i in 0..n {
            a.d[i] = a.d[i] * (1.0 - blend) + fa.d[i] * blend;
            b.d[i] = b.d[i] * (1.0 - blend) + fb.d[i] * blend;
        }
        report.add("colour noise",
            json!({"applied": true, "radius": radius, "blend": round_to(blend, 3)}));
    } else {
        report.add("colour noise", json!({"applied": false,
            "reason": if !s.on("denoise_chroma") { "switched_off" } else { "clean_enough" },
            "noise": round_to(noise, 3)}));
    }

    // --- luminance noise --------------------------------------------------
    if s.on("denoise_luma") && p.denoise && noise > 0.30 {
        let radius = ((img.w.max(img.h) as f32 / 900.0).round() as usize).max(2);
        let blend = ((noise - 0.30) / 0.6).clamp(0.0, 1.0) * 0.55;
        let fl = ops::guided_filter(&light_new, &light_new, radius, 2.5e-4, 2);
        for i in 0..n {
            light_new.d[i] = light_new.d[i] * (1.0 - blend) + fl.d[i] * blend;
        }
        report.add("luminance noise",
            json!({"applied": true, "radius": radius, "blend": round_to(blend, 3)}));
    } else {
        report.add("luminance noise", json!({"applied": false,
            "reason": if !s.on("denoise_luma") { "switched_off" } else { "clean_enough" },
            "noise": round_to(noise, 3)}));
    }

    // --- vibrance ---------------------------------------------------------
    let mut chroma = Plane::new(img.w, img.h);
    for i in 0..n {
        chroma.d[i] = a.d[i].hypot(b.d[i]);
    }
    let view_c = stats_view_plane(&chroma, stats_rect);
    let view_l = stats_view_plane(&light_new, stats_rect);
    let (tw, th) = thumb_size(view_c.w, view_c.h);
    let (mut boost, mut info) = analyze::auto_vibrance(
        &ops::resize_plane(&view_c, tw, th),
        &ops::resize_plane(&view_l, tw, th),
        p.target_chroma,
        p.max_boost,
    );
    // Chroma noise rides up with the colour.
    let restraint = (1.0 - 0.6 * noise).clamp(0.3, 1.0);
    boost = 1.0 + (boost - 1.0) * restraint;
    if let Some(o) = info.as_object_mut() {
        o.insert("noise_restraint".into(), json!(round_to(restraint, 3)));
    }
    boost = (boost * (1.0 + 0.55 * s.vibrance)).clamp(0.45, 2.4);
    if let Some(o) = info.as_object_mut() {
        o.insert("user".into(), json!(round_to(s.vibrance, 3)));
        o.insert("final_boost".into(), json!(round_to(boost, 3)));
    }
    if !s.on("vibrance") {
        let mut off = info.clone();
        if let Some(o) = off.as_object_mut() {
            o.insert("applied".into(), json!(false));
            o.insert("reason".into(), json!("switched_off"));
        }
        report.add("vibrance", off);
        boost = 1.0;
    } else {
        report.add("vibrance", info);
    }
    if (boost - 1.0).abs() > 0.001 {
        let skin = skin_hue();
        for i in 0..n {
            let c = chroma.d[i];
            let hue = b.d[i].atan2(a.d[i]);
            let mut weight = 1.0 - ops::smoothstep(0.10, 0.30, c);
            weight *= 1.0 - 0.55 * skin_protection(hue, c, skin);
            let gain = 1.0 + (boost - 1.0) * weight;
            a.d[i] *= gain;
            b.d[i] *= gain;
        }
    }
    drop(chroma);

    // --- sharpening -------------------------------------------------------
    let amount = p.sharpen * (1.0 - 0.75 * noise) * s.render_scale.clamp(0.25, 1.0);
    if s.on("sharpen") && amount > 0.02 {
        let blurred = ops::gaussian_blur(&light_new, sharpen_sigma);
        let floor = 0.004 + 0.02 * noise;
        for i in 0..n {
            let mut detail = light_new.d[i] - blurred.d[i];
            detail *= ops::smoothstep(floor, floor * 3.0, detail.abs());
            light_new.d[i] = (light_new.d[i] + (detail * amount).clamp(-0.07, 0.07)).clamp(0.0, 1.0);
        }
        report.add("sharpening", json!({"applied": true, "amount": round_to(amount, 3),
            "radius_px": round_to(sharpen_sigma, 2), "noise_floor": round_to(floor, 4)}));
    } else {
        report.add("sharpening", json!({"applied": false,
            "reason": if !s.on("sharpen") { "switched_off" } else { "too_noisy" },
            "noise": round_to(noise, 3)}));
    }

    to_gamut(&light_new, &a, &b)
}

/// Oklab -> linear sRGB, pulling chroma back until each pixel fits.
pub fn to_gamut(lightness: &Plane, a: &Plane, b: &Plane) -> Image {
    let mut out = Image::new(lightness.w, lightness.h);
    for i in 0..lightness.d.len() {
        let l = lightness.d[i].clamp(0.0, 1.0);
        let (mut r, mut g, mut bl) = ops::oklab_to_rgb_px(l, a.d[i], b.d[i]);
        if r.min(g).min(bl) < -0.001 || r.max(g).max(bl) > 1.001 {
            let (mut lo, mut hi) = (0.0f32, 1.0f32);
            for _ in 0..10 {
                let mid = 0.5 * (lo + hi);
                let (tr, tg, tb) = ops::oklab_to_rgb_px(l, a.d[i] * mid, b.d[i] * mid);
                if tr.min(tg).min(tb) >= -0.001 && tr.max(tg).max(tb) <= 1.001 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let (fr, fg, fb) = ops::oklab_to_rgb_px(l, a.d[i] * lo, b.d[i] * lo);
            r = fr;
            g = fg;
            bl = fb;
        }
        out.d[i * 3] = r.clamp(0.0, 1.0);
        out.d[i * 3 + 1] = g.clamp(0.0, 1.0);
        out.d[i * 3 + 2] = bl.clamp(0.0, 1.0);
    }
    out
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
    pub preset: Option<Preset>,
    /// The noise floor measured once at full resolution.
    pub native_noise_floor: Option<f32>,
    pub progress: Option<&'a mut dyn FnMut(f32, &str)>,
}

/// Linear scene-referred RGB -> display-referred linear sRGB in [0,1].
pub fn process(src: &Image, settings: &Settings, args: ProcessArgs, report: &mut Report) -> Image {
    let p = args.preset.unwrap_or_else(|| settings.preset_obj());
    let mut img = src.clone();
    let mut progress = args.progress;
    let mut mark = 0usize;
    let step = |progress: &mut Option<&mut dyn FnMut(f32, &str)>, mark: &mut usize| {
        if let Some(cb) = progress {
            let (f, code) = STAGE_MARKS[*mark];
            cb(f, code);
        }
        *mark += 1;
    };

    let measure = |img: &Image, rect: Option<Rect>| ops::thumbnail(&stats_view_img(img, rect), 1024);

    step(&mut progress, &mut mark);
    let thumb = measure(&img, args.stats_rect);
    apply_white_balance(&mut img, &thumb, settings, report);
    step(&mut progress, &mut mark);
    let thumb = measure(&img, args.stats_rect);
    let exposure_gain = apply_exposure(&mut img, &thumb, settings, &p, report);
    step(&mut progress, &mut mark);

    let thumb = measure(&img, args.stats_rect);
    // Noise must be measured at native resolution.
    let (noise, info) = match args.native_noise_floor {
        Some(floor) => analyze::noise_from_floor(floor * exposure_gain.max(1e-6).sqrt(),
                                                 args.iso, args.noise_prior),
        None => {
            let view = stats_view_img(&img, args.stats_rect);
            let crop = centre_crop(&view, 640);
            let mut sqrt_luma = ops::luminance(&crop);
            for v in sqrt_luma.d.iter_mut() {
                *v = v.max(0.0).sqrt();
            }
            analyze::estimate_noise(&sqrt_luma, args.iso, args.noise_prior)
        }
    };
    report.add("noise", info);

    step(&mut progress, &mut mark);
    let compression = apply_local_tone_map(&mut img, &thumb, settings, &p, report);
    step(&mut progress, &mut mark);
    filmic(&mut img);
    step(&mut progress, &mut mark);
    let out = apply_perceptual(&img, settings, &p, noise, args.sharpen_sigma, compression,
                               report, args.stats_rect);
    step(&mut progress, &mut mark);
    out
}

/// Toggle metadata for the interface, mirroring `develop.toggles`.
pub fn toggle_list() -> Vec<Map<String, Value>> {
    TOGGLES
        .iter()
        .map(|(id, label, group)| {
            let mut m = Map::new();
            m.insert("id".into(), json!(id));
            m.insert("label".into(), json!(label));
            m.insert("group".into(), json!(group));
            m
        })
        .collect()
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

    /// Out-of-gamut colours lose chroma at constant lightness rather than
    /// being clipped per channel, which is what keeps gradients and hue.
    #[test]
    fn to_gamut_preserves_lightness_and_hue() {
        let l = Plane::filled(1, 1, 0.6);
        let a = Plane::filled(1, 1, 0.35); // far outside sRGB
        let b = Plane::filled(1, 1, 0.05);
        let img = to_gamut(&l, &a, &b);
        let (gl, ga, gb) = ops::rgb_to_oklab_px(img.d[0], img.d[1], img.d[2]);
        assert!((gl - 0.6).abs() < 0.02, "lightness moved: {}", gl);
        let hue_in = (0.05f32).atan2(0.35);
        let hue_out = gb.atan2(ga);
        assert!((hue_in - hue_out).abs() < 0.05, "hue moved: {} -> {}", hue_in, hue_out);
        assert!(img.d.iter().all(|v| *v >= 0.0 && *v <= 1.0));
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

    #[test]
    fn settings_parse_from_the_wire() {
        let v = serde_json::json!({
            "framing": {"angle": 2.5, "auto_fit": false, "crop": [0.1, 0.1, 0.5, 0.5]},
            "vibrance": 0.4, "shadows": 0.6, "midtones": -0.2, "highlights": 4.0,
            "preset": "punchy", "enabled": {"sharpen": false}});
        let s = Settings::from_json(&v);
        assert_eq!(s.shadows, 0.6);
        assert_eq!(s.midtones, -0.2);
        assert_eq!(s.highlights, 1.0, "out-of-range slider was not clamped");
        assert_eq!(s.framing.angle, 2.5);
        assert_eq!(s.framing.auto_fit, false);
        assert_eq!(s.framing.crop, Some((0.1, 0.1, 0.5, 0.5)));
        assert!(!s.on("sharpen"));
        assert!(s.on("contrast"));
        assert_eq!(s.preset_obj().name, "punchy");
    }
}

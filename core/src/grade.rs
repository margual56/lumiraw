//! The grading pipeline: linear raw in, display-ready sRGB out.

use crate::analyze;
use crate::curve;
use crate::geometry::{in_frame, stats_view_img, stats_view_plane, Framing, Rect};
use kit::{Image, Lab, Plane, Report, round_to, with};
use serde_json::{json, Value};
use std::collections::HashMap;

pub const EPS: f32 = 1e-6;

/// The most the automatic white balance may move one channel, in stops.
pub const WB_LIMIT_EV: f32 = 0.8;

/// How much of vibrance and the wheels is kept off skin unless asked otherwise.
pub const SKIN_PROTECT: f32 = 0.55;

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
    /// Chroma gain on every colour, a look's own saturation, as a camera's
    /// picture style has; vibrance (`target_chroma`) then lifts what is muted.
    pub saturation: f32,
    pub denoise: bool,
    /// How much of what the highlight guard held back the tone map gives back
    /// to the rest of the frame: half of it by day (all of it brightened every
    /// frame a stop past the camera's own rendering), none of it by night,
    /// when dark is what the picture is (see `profile::tune`).
    pub give_back: f32,
    /// The tone map brings the highlights under the shoulder however far it
    /// has to squeeze: for a merged bracket, which was shot to keep both its
    /// shadows and its highlights.
    pub keep_highlights: bool,
    /// Where the frame's highlights stop counting for the automatic white
    /// balance: 1, except for a merged bracket (see `profile::tune`).
    pub wb_ceiling: f32,
    /// For a merged bracket, the faintest light its brightest frame saw clear
    /// of its noise, in the merged frame's values; zero for any other frame.
    /// With `keep_highlights`, what `Vouched` holds the development to.
    pub bracket_floor: f32,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            name: "natural".into(),
            wb_strength: 1.0,
            exposure_strength: 1.0,
            target_key: 0.10,
            comfortable_stops: 6.5,
            detail_boost: 1.15,
            levels_strength: 0.60,
            contrast_amount: 0.55,
            target_chroma: 0.078,
            max_boost: 1.55,
            sharpen: 0.55,
            saturation: 1.10,
            denoise: true,
            give_back: 0.5,
            keep_highlights: false,
            wb_ceiling: 1.0,
            bracket_floor: 0.0,
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
            contrast_amount: 0.70,
            target_chroma: 0.095,
            max_boost: 1.8,
            sharpen: 0.75,
            saturation: 1.2,
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
            saturation: 1.0,
            ..base
        },
        _ => base,
    }
}

/// Every automatic correction the user is allowed to switch off, in the order
/// the pipeline applies them.
pub const TOGGLES: [(&str, &str, &str, &str); 13] = [
    ("lens_vignetting", "Vignetting", "Lens", "vignetting"),
    ("lens_distortion", "Distortion", "Lens", "distortion"),
    ("lens_tca", "Chromatic aberration", "Lens", "chromatic aberration"),
    ("white_balance", "White balance", "Colour", "white balance"),
    ("exposure", "Exposure", "Tone", "exposure"),
    ("tone_map", "Local tone mapping", "Tone", "tone mapping"),
    ("levels", "Black & white point", "Tone", "levels"),
    ("contrast", "Contrast", "Tone", "contrast"),
    ("vibrance", "Vibrance", "Colour", "vibrance"),
    ("denoise_chroma", "Colour noise", "Detail", "colour noise"),
    ("denoise_luma", "Luminance noise", "Detail", "luminance noise"),
    ("refocus", "Focus recovery", "Detail", "focus recovery"),
    ("sharpen", "Sharpening", "Detail", "sharpening"),
];

/// What the automatic stages decided for a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Decisions {
    /// The white balance gains `adapt` applied, before temperature and tint.
    pub wb: Option<[f32; 3]>,
    /// The whole exposure gain, the photographer's bias included.
    pub exposure: f32,
    /// The tone map's compression, and the base level it pivots on (log2).
    pub tone: (f32, f32),
    /// The tone map's shoulder, when it has one.
    pub shoulder: Option<Shoulder>,
    /// Black and white point, when levels were applied.
    pub levels: Option<(f32, f32)>,
    /// The contrast curve's amount and pivot.
    pub contrast: (f32, f32),
    /// The vibrance boost as applied, one when it was not.
    pub vibrance: f32,
    /// The colour of the air dehazing measured, when it ran.
    pub air: Option<[f32; 3]>,
    /// How many stops the highlight guard held the exposure back: the tone map
    /// gives them back to everything below the highlights.
    pub held_back: f32,
    /// What the tone map gave back of it: a lift of this many stops (log2) at
    /// and below the first base level, fading to none at the second.
    pub lift: (f32, f32, f32),
}

impl Default for Decisions {
    fn default() -> Self {
        Decisions { wb: None, exposure: 1.0, tone: (1.0, 0.0), shoulder: None, levels: None,
                    contrast: (0.0, 0.5), vibrance: 1.0, air: None, held_back: 0.0,
                    lift: (0.0, 0.0, 0.0) }
    }
}

/// Where an image sits in the frame it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Where the image's top-left pixel is in the frame.
    pub x: usize,
    pub y: usize,
    /// The whole frame's size.
    pub w: usize,
    pub h: usize,
}

impl Window {
    pub fn whole(img: &Image) -> Window {
        Window { x: 0, y: 0, w: img.w, h: img.h }
    }
    pub fn long_edge(&self) -> usize {
        self.w.max(self.h)
    }
    /// The frame as fractions of this image, for the stages that place
    /// things in the frame (local filters): None when the image is all of it.
    pub fn frame(&self, img: &Image) -> Option<Rect> {
        if self.x == 0 && self.y == 0 && self.w == img.w && self.h == img.h {
            return None;
        }
        let (iw, ih) = (img.w as f32, img.h as f32);
        Some((-(self.x as f32) / iw, -(self.y as f32) / ih, self.w as f32 / iw, self.h as f32 / ih))
    }
}

/// Everything the user chose.  `None` means "decide it automatically".
#[derive(Clone, Debug, PartialEq)]
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
    /// Haze taken out (up to 1) or put in (down to -1); see `dehaze`.
    pub dehaze: f32,
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
    /// Colour grading wheels; see `wheels.rs`.
    pub wheels: crate::wheels::Wheels,
    /// What skin looks like here, and how much of vibrance and the wheels to
    /// keep off it, 0..1.
    pub skin: Skin,
    pub skin_protect: f32,
    /// How much of the loaded 3D table to apply, 0..1.
    pub lut: f32,
    /// The three things a curve cannot do, each 0..1. See `effects.rs`.
    pub monochrome: f32,
    pub vignette: f32,
    pub grain: f32,
    /// The lens to correct for: empty to find it from the file, "camera" for
    /// the corrections the camera wrote, or a lens's model in the database.
    pub lens: String,
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
            dehaze: 0.0,
            denoise: 0.0,
            filters: Vec::new(),
            spots: Vec::new(),
            preset: "natural".into(),
            curves: curve::Stack::identity(),
            mixer: crate::mixer::Mixer::default(),
            wheels: crate::wheels::Wheels::default(),
            skin: Skin::default(),
            skin_protect: SKIN_PROTECT,
            lut: 0.0,
            monochrome: 0.0,
            vignette: 0.0,
            grain: 0.0,
            lens: String::new(),
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
        s.dehaze = f("dehaze").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.denoise = f("denoise").unwrap_or(0.0).clamp(-1.0, 1.0);
        s.filters = crate::local::filters_from_json(obj.get("filters"));
        s.spots = crate::local::spots_from_json(obj.get("spots"));
        if let Some(p) = obj.get("preset").and_then(|x| x.as_str()) {
            s.preset = p.to_string();
        }
        if let Some(l) = obj.get("lens").and_then(|x| x.as_str()) {
            s.lens = l.to_string();
        }
        s.curves = curve::Stack::from_json(obj.get("curves"));
        s.mixer = crate::mixer::Mixer::from_json(obj.get("mixer"));
        s.wheels = crate::wheels::Wheels::from_json(obj.get("wheels"));
        s.skin = Skin::from_json(obj.get("skin"));
        s.skin_protect = f("skin_protect").unwrap_or(SKIN_PROTECT).clamp(0.0, 1.0);
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

/// What skin looks like in this photograph, so the stages that move colour
/// can leave faces alone: an Oklab hue and chroma, measured from the faces in
/// the frame (see `skin`), a typical skin until the frame has been looked at,
/// and none once it has and nobody is in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub hue: f32,
    pub chroma: f32,
    pub source: SkinSource,
}

/// Where the skin being protected comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SkinSource {
    /// Nobody has looked for faces yet: a typical skin, broadly.
    Typical,
    /// Read off this photograph's faces.
    Measured,
    /// Looked for and none found: a landscape's oranges are not skin.
    NoFaces,
}

impl Default for Skin {
    fn default() -> Self {
        let lin = [0.76f32, 0.57, 0.46].map(kit::srgb_decode_scalar);
        let (_, a, b) = kit::rgb_to_oklab_px(lin[0], lin[1], lin[2]);
        Skin { hue: b.atan2(a), chroma: a.hypot(b), source: SkinSource::Typical }
    }
}

impl Skin {
    pub fn from_json(v: Option<&Value>) -> Skin {
        let read = |k: &str| v.and_then(|v| v.get(k)).and_then(|x| x.as_f64()).map(|x| x as f32);
        match (read("hue"), read("chroma")) {
            (Some(hue), Some(chroma)) if chroma > 0.005 =>
                Skin { hue, chroma: chroma.min(0.3), source: SkinSource::Measured },
            _ if read("faces") == Some(0.0) => Skin { source: SkinSource::NoFaces, ..Skin::default() },
            _ => Skin::default(),
        }
    }

    /// How much a colour is this skin, 0..1. A measured skin is a narrower
    /// target than the typical one: it is known, so less has to be guessed.
    pub fn likeness(&self, hue: f32, chroma: f32) -> f32 {
        let mut d = (hue - self.hue).abs();
        while d > std::f32::consts::PI {
            d = (std::f32::consts::TAU - d).abs();
        }
        match self.source {
            SkinSource::NoFaces => return 0.0,
            SkinSource::Typical => return (1.0 - kit::smoothstep(0.25, 0.60, d))
                * (1.0 - kit::smoothstep(0.10, 0.20, chroma)),
            SkinSource::Measured => {}
        }
        let c = self.chroma;
        (1.0 - kit::smoothstep(0.15, 0.40, d))
            * kit::smoothstep(0.25 * c, 0.55 * c, chroma)
            * (1.0 - kit::smoothstep(1.6 * c, 2.6 * c, chroma))
    }
}

// Region sampling
/// Pixels inside a rectangle given in fractions of the kept `frame`, with blown
/// ones dropped.
pub fn sample_rect(img: &Image, rect: Rect, frame: Option<Rect>) -> Option<Vec<[f32; 3]>> {
    let (w, h) = (img.w as f32, img.h as f32);
    let (x, y, rw, rh) = in_frame(rect, frame);
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

// Scene-linear stages
/// White balance, on the frame and on its measuring thumbnail alike.
pub fn apply_white_balance(img: &mut Image, thumb: &mut Image, s: &Settings, p: &Preset,
                           frame: Option<Rect>, d: &mut Decisions, frozen: bool,
                           report: &mut Report) {
    if frozen {
        if let Some(gains) = d.wb {
            adapt(img, gains);
        }
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    d.wb = None;
    if !s.on("white_balance") {
        report.skipped("white balance", "switched_off", json!({}));
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    if let Some(patch) = s.wb_rect.and_then(|r| sample_rect(img, r, frame)).filter(|p| p.len() >= 4) {
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
        d.wb = Some(gains);
        adapt(img, gains);
        adapt(thumb, gains);
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    // The tuned preset, not the named one: the capture decides how far the
    // automatic balance may be trusted (see `profile::tune`), and not at all
    // when the camera balanced the shot itself.
    if p.wb_strength <= 0.0 {
        report.skipped("white balance", "camera_balanced", json!({"source": "automatic"}));
        apply_temp_tint(img, thumb, s, report);
        return;
    }
    let (gains, info) = analyze::auto_white_balance(thumb, p.wb_strength, WB_LIMIT_EV,
                                                    p.wb_ceiling);
    let info = with(info, json!({"source": "automatic", "strength": round_to(p.wb_strength, 2)}));
    let applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
    report.add("white balance", info);
    if applied {
        d.wb = Some(gains);
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
///
/// A pixel blown in every channel holds no colour to correct: the decoder
/// made it white, and white it stays, at the brightness the correction gives
/// it. Corrected as a colour, a blown sky or sun would take the opposite of
/// the correction's tint.
pub fn adapt(img: &mut Image, gains: [f32; 3]) {
    if gains.iter().all(|g| (g - 1.0).abs() < 1e-6) {
        return;
    }
    let white = [1.0 / gains[0].max(1e-6), 1.0 / gains[1].max(1e-6), 1.0 / gains[2].max(1e-6)];
    let m = kit::bradford_adaptation(white);
    for px in img.d.chunks_exact_mut(3) {
        let blown = kit::smoothstep(0.95, 0.99, px[0].min(px[1]).min(px[2]));
        let out = m.apply([px[0], px[1], px[2]]);
        let y = kit::luminance_px(&out);
        for c in 0..3 {
            px[c] = out[c] + (y - out[c]) * blown;
        }
    }
}

pub fn apply_exposure(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                      frame: Option<Rect>, d: &mut Decisions, frozen: bool,
                      report: &mut Report) -> f32 {
    let gain = if frozen {
        d.exposure
    } else {
        let (gain, held) = decide_exposure(img, thumb, s, p, frame, report);
        d.held_back = held;
        gain
    };
    d.exposure = gain;
    img.scale(gain);
    gain
}

/// The exposure gain, and how many stops the highlight guard held it back.
fn decide_exposure(img: &Image, thumb: &Image, s: &Settings, p: &Preset, frame: Option<Rect>,
                   report: &mut Report) -> (f32, f32) {
    if !s.on("exposure") {
        report.skipped("exposure", "switched_off", json!({}));
        return (s.exposure_bias.exp2(), 0.0);
    }
    if let Some(patch) = s.exposure_rect.and_then(|r| sample_rect(img, r, frame)) {
        // Geometric mean: a few bright specks in the selection should not
        // drag the whole frame down.
        let log_sum: f64 = patch
            .iter()
            .map(|px| kit::luminance_px(px).max(EPS).log2() as f64)
            .sum();
        let level = ((log_sum / patch.len() as f64) as f32).exp2();
        let gain = (p.target_key / level).clamp(2f32.powi(-6), 2f32.powi(8));
        // What the selection decided, without the photographer's own bias,
        // the same as the automatic path reports it.
        report.add("exposure", json!({"applied": true, "source": "selection",
            "patch_level": round_to(level, 5), "gain_ev": round_to(gain.log2(), 3),
            "bias_ev": round_to(s.exposure_bias, 2)}));
        return (gain * s.exposure_bias.exp2(), 0.0);
    }
    let (mut gain, info) =
        analyze::auto_exposure(thumb, p.target_key, p.exposure_strength, 3.2);
    gain *= s.exposure_bias.exp2();
    let held = info.get("highlight_guard_ev").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    report.add("exposure", with(info, json!({"source": "automatic",
                                              "bias_ev": round_to(s.exposure_bias, 2)})));
    (gain, held)
}

/// Haze out or in, on the frame and on the thumbnail the tone map measures.
fn apply_dehaze(img: &mut Image, thumb: &mut Image, s: &Settings, long_edge: usize,
                d: &mut Decisions, frozen: bool, report: &mut Report) {
    if s.dehaze.abs() < 1e-3 {
        d.air = None;
        report.skipped("dehaze", "not_asked", json!({}));
        return;
    }
    let air = match (frozen, d.air) {
        (true, Some(air)) => air,
        _ => crate::dehaze::air_light(thumb),
    };
    d.air = Some(air);
    crate::dehaze::apply(img, air, s.dehaze, long_edge);
    if !frozen {
        let edge = thumb.w.max(thumb.h);
        crate::dehaze::apply(thumb, air, s.dehaze, edge);
    }
    report.add("dehaze", json!({"applied": true, "amount": round_to(s.dehaze, 2),
                                "air": air.map(|v| round_to(v, 4))}));
}

/// Compress the scene's range on the low-frequency layer only.
pub fn apply_local_tone_map(img: &mut Image, thumb: &Image, s: &Settings, p: &Preset,
                            long_edge: usize, d: &mut Decisions, frozen: bool,
                            held: Option<&[u8]>, report: &mut Report) -> f32 {
    let (mut factor, info) = if frozen {
        (d.tone.0, json!({}))
    } else {
        analyze::auto_tone_compression(thumb, p.comfortable_stops, TONE_FLOOR)
    };
    // Clarity scales the fine-structure layer of this same split.
    let detail_boost = p.detail_boost * (1.0 + 0.6 * s.clarity);
    let info = with(info, json!({"detail_boost": round_to(detail_boost, 3),
                                 "clarity": round_to(s.clarity, 2)}));
    if !s.on("tone_map") {
        d.tone.0 = factor;
        report.skipped("tone mapping", "switched_off", info);
        return 1.0;
    }

    let y = kit::luminance(img);
    let mut log_y = Plane::new(y.w, y.h);
    for i in 0..y.d.len() {
        log_y.d[i] = kit::fast_log2(y.d[i].max(1e-5));
    }
    let radius = ((long_edge as f32 / 45.0).round() as usize).max(8);
    let base = kit::guided_filter(&log_y, &log_y, radius, 0.15, 4);

    // Anchor on the 55th percentile of the base layer, every 4th pixel.
    let mut lift = 0.0;
    // The brightest large-scale light (log2, before compression) a merged
    // bracket's shoulder is fitted to.
    let mut bright = None;
    let anchor = if frozen {
        d.tone.1
    } else {
        d.lift = (0.0, 0.0, 0.0);
        let mut sample: Vec<f32> = Vec::with_capacity(base.d.len() / 16 + 1);
        for yy in (0..base.h).step_by(4) {
            for xx in (0..base.w).step_by(4) {
                sample.push(base.d[yy * base.w + xx]);
            }
        }
        let q = kit::percentiles(&sample, &[(analyze::CRUSH_BUDGET * 100.0) as f64, 55.0, 99.5,
                                            SHOULDER_FITS_PCT]);
        let (low, middle, high) = (q[0], q[1], q[2]);
        bright = Some(q[3]);
        let mut anchor = middle;
        // The stops the highlight guard held the exposure back are given back
        // to the middle of the frame here, so a room is not left dark for its
        // window: a lift of that much at and below the middle, fading to none
        // at the highlights, which stay where the guard put them. The shadows
        // rise with the middle and no further, so what was black stays black
        // against it; and no more is given than keeps a third of the contrast
        // between the middle and the highlights.
        let held = (d.held_back * p.give_back).clamp(0.0, analyze::MAX_RESCUE_EV);
        if held > 0.0 && high > middle {
            d.lift = (held.min((1.0 - HELD_FLOOR) * (high - middle)), middle, high);
        }
        // Shadows still below the display's floor are lifted the same way:
        // the pivot moves up toward the highlights, so everything below it
        // rises; then, if that is not enough, the compression deepens.
        let need = (analyze::DISPLAY_FLOOR.log2() - low).clamp(0.0, analyze::MAX_RESCUE_EV);
        if need > 0.0 && high > low {
            (anchor, factor) = lift_shadows(low, anchor, high, factor, need);
            lift = (anchor - low) * (1.0 - factor);
        }
        anchor
    };
    d.tone = (factor, anchor);
    let (given, from, to) = d.lift;
    let info = with(info, json!({"compression": round_to(factor, 3),
                                 "shadow_lift_ev": round_to(lift, 2),
                                 "held_back_ev": round_to(d.held_back, 2),
                                 "given_back_ev": round_to(given, 2)}));
    // Compression keeps the order of the base levels, so the percentile of the
    // compressed light is the compressed percentile.
    let shoulder = match bright {
        Some(b) if p.keep_highlights => Shoulder::fitting(anchor + (b - anchor) * factor),
        Some(_) => None,
        None => d.shoulder,
    };
    d.shoulder = shoulder;
    let info = match shoulder {
        Some(s) => with(info, json!({"shoulder_knee": round_to(s.knee.exp2(), 3),
                                     "shoulder_bend": round_to(s.bend, 3)})),
        None => info,
    };
    report.add("tone mapping", info);
    if factor >= 0.995 && (detail_boost - 1.0).abs() < 1e-3 && given <= 0.0
        && shoulder.is_none() && held.is_none() {
        return factor;
    }
    let ceiling = HeldCeiling::new();

    for (i, px) in img.px_mut().iter_mut().enumerate() {
        let detail = log_y.d[i] - base.d[i];
        let given_here = if given > 0.0 {
            given * (1.0 - kit::smoothstep(from, to, base.d[i]))
        } else {
            0.0
        };
        let level = anchor + (base.d[i] - anchor) * factor + given_here;
        let mut new_log = shoulder.map_or(level, |s| s.apply(level)) + detail * detail_boost;
        // What the dark frames held is rolled off under white here, detail
        // and all, rather than flattened there by the curve after: a street
        // light rides on the detail layer, which the shoulder never sees.
        if let Some(w) = held.map(|h| h[i] as f32 / 255.0).filter(|w| *w > 0.0) {
            let top = (px[0].max(px[1]).max(px[2]).max(EPS)).log2() + new_log - log_y.d[i];
            new_log -= w * (top - ceiling.apply(top));
        }
        let scale = kit::fast_exp2(new_log - log_y.d[i]);
        for v in px.iter_mut() {
            *v *= scale;
        }
    }
    factor
}

/// Where the tone map pivots, and how hard it compresses, to lift shadows at
/// `low` by `need` stops (all in log2 of the base layer): the pivot moves up
/// from `anchor` toward the highlights at `high`, so everything below it
/// rises; if even the highlights are not high enough, the compression
/// deepens.
fn lift_shadows(low: f32, anchor: f32, high: f32, factor: f32, need: f32) -> (f32, f32) {
    let reach = |anchor: f32| (anchor - low) * (1.0 - factor);
    if reach(anchor) >= need {
        return (anchor, factor);
    }
    // The pivot that lifts them exactly that far. Decided on where it falls,
    // not on its lift recomputed, which rounding can put a hair short.
    let wanted = if factor < 0.999 { low + need / (1.0 - factor) } else { f32::INFINITY };
    if wanted <= high {
        return (wanted, factor);
    }
    (high, factor.min(1.0 - need / (high - low)).max(TONE_FLOOR))
}

/// The brightest a channel may be taken by vibrance and saturation, short of
/// white, so that a boosted gradient keeps its steps.
const BOOST_HEADROOM: f32 = 0.97;

/// The most the tone map compresses a scene's range.
const TONE_FLOOR: f32 = 0.45;

/// What the tone map keeps of the contrast between the middle and the
/// highlights when it gives back what the highlight guard held back.
const HELD_FLOOR: f32 = 0.65;

/// The filmic curve, y = x (A x + B) / (C x² + D x + E).
const FILMIC: [f32; 5] = [2.51, 0.03, 2.43, 0.59, 0.14];

#[inline]
fn curve(x: f32) -> f32 {
    let [a, b, c, d, e] = FILMIC;
    let x = x.max(0.0);
    ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
}

/// How much a curve laid on each channel, as a camera lays its own, would
/// deepen a colour `filmic` has laid it on by its brightest channel: the
/// factor its chroma for its lightness takes. The curve raises each ratio
/// to the brightest channel to its log-slope γ (d ln y / d ln x) there,
/// which widens the spread between the channels for their luminance; that
/// is where a camera's colour gets its depth through the shadows and the
/// middle, and without it they come out a third greyer than the camera's.
/// One where the curve flattens, toward the shoulder (the bleaching there
/// is `filmic`'s), for a grey, and for a colour with a channel below zero,
/// outside sRGB, which has no ratio to raise.
fn filmic_depth(px: [f32; 3]) -> f32 {
    let [a, b, c, d, e] = FILMIC;
    let (top, weakest) = (px[0].max(px[1]).max(px[2]), px[0].min(px[1]).min(px[2]));
    if weakest <= 0.0 || top >= 1.0 || weakest >= top {
        return 1.0;
    }
    // The scene level the curve took to `top`: the positive root of
    // (top C - A) x² + (top D - B) x + top E = 0.
    let (qa, qb, qc) = (top * c - a, top * d - b, top * e);
    let x = (-qb - (qb * qb - 4.0 * qa * qc).max(0.0).sqrt()) / (2.0 * qa);
    let gamma = 1.0 + a * x / (a * x + b) - x * (2.0 * c * x + d) / (x * (c * x + d) + e);
    if gamma <= 1.0 {
        return 1.0;
    }
    // r^γ = r r^(γ-1), to first order in γ - 1 (at most 0.6 here).
    let steep = px.map(|v| v * (1.0 - (gamma - 1.0) * (1.0 - v / top)));
    let weakest_steep = steep[0].min(steep[1]).min(steep[2]);
    let (y, y_steep) = (kit::luminance_px(&px), kit::luminance_px(&steep));
    ((top - weakest_steep) * y) / ((top - weakest) * y_steep).max(EPS)
}

/// What share of the large-scale light a merged bracket's shoulder brings
/// under `SHOULDER_TOP`.
const SHOULDER_FITS_PCT: f64 = 99.0;
/// Where the shoulder starts, in scene light after exposure: a little above
/// middle grey, so the shadows and midtones stay as the exposure set them.
const SHOULDER_KNEE: f32 = 0.3;
/// Where it brings the bright areas to: the filmic curve shows 1 at about
/// 80 %, which leaves room for the detail riding on them.
const SHOULDER_TOP: f32 = 0.5;

/// A roll-off on the tone map's large-scale light above a knee, for a merged
/// bracket: the exposure is set for the shadows the bright frames held, and
/// this brings down the highlights the dark frames held, with their detail
/// put back on top like everywhere else. In stops, `x` above the knee goes
/// to `x / (1 + bend x)`: untouched at the knee, and smooth through it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shoulder {
    knee: f32,
    bend: f32,
}

impl Shoulder {
    /// The shoulder that brings `bright` (log2) to the top, or none when it
    /// is there already.
    fn fitting(bright: f32) -> Option<Shoulder> {
        let (knee, top) = (SHOULDER_KNEE.log2(), SHOULDER_TOP.log2());
        let (x, t) = (bright - knee, top - knee);
        (x > t).then(|| Shoulder { knee, bend: (x / t - 1.0) / x })
    }

    fn apply(&self, level: f32) -> f32 {
        let x = level - self.knee;
        if x <= 0.0 { level } else { self.knee + x / (1.0 + self.bend * x) }
    }
}

/// Where a merged bracket's frames vouch for a pixel, from its merged value:
/// how far it is below the darkest frame's ceiling (`held`), and above the
/// brightest frame's noise (`seen`), each 0..255. A held pixel must not come
/// out clipped, nor a seen one black: what one of the frames showed, the
/// merge shows too.
#[derive(Clone)]
pub struct Vouched {
    pub w: usize,
    pub held: Vec<u8>,
    pub seen: Vec<u8>,
}

/// Below this share of the darkest frame's ceiling a merged value was held.
const HELD_BELOW: f32 = 0.95;

impl Vouched {
    /// The weights as an image, `[held, seen, 0]`, from a merged frame's own
    /// values (in the darkest frame's light, so 1 is its ceiling) and the
    /// brightest frame's noise floor in them (`Preset::bracket_floor`). An
    /// image, so that it can go through the same lens geometry and framing
    /// as the frame: measured after them, a corrected vignette would have
    /// moved the ceiling.
    pub fn marks(img: &Image, floor: f32) -> Image {
        let mut out = Image::new(img.w, img.h);
        for (m, px) in out.px_mut().iter_mut().zip(img.px()) {
            let top = px[0].max(px[1]).max(px[2]);
            // Fully seen from half the floor: the merge averages the darker
            // frames' readings in, which can leave it a little under what the
            // brightest frame alone saw.
            *m = [1.0 - kit::smoothstep(HELD_BELOW, 1.0, top),
                  kit::smoothstep(0.25 * floor, 0.5 * floor, top), 0.0];
        }
        out
    }

    /// The weights kept, a byte each, once the marks have been through
    /// whatever moves the frame.
    pub fn from_marks(marks: &Image) -> Vouched {
        let weight = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Vouched {
            w: marks.w,
            held: marks.px().iter().map(|m| weight(m[0])).collect(),
            seen: marks.px().iter().map(|m| weight(m[1])).collect(),
        }
    }

    /// The weights of a piece of the frame, as `Image::crop` cuts it.
    pub fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Vouched {
        let rows = |v: &[u8]| (y..y + h).flat_map(|r| v[r * self.w + x..][..w].iter().copied())
            .collect();
        Vouched { w, held: rows(&self.held), seen: rows(&self.seen) }
    }

    /// For a merged frame (`Preset::keep_highlights`) straight from the merge,
    /// with no lens correction or framing between; none for any other frame.
    pub fn measure(img: &Image, p: &Preset) -> Option<Vouched> {
        (p.keep_highlights && p.bracket_floor > 0.0)
            .then(|| Vouched::from_marks(&Vouched::marks(img, p.bracket_floor)))
    }

    /// Hold a finished development, display-linear, to what the frames
    /// vouched for: a held pixel's channels under white, a seen pixel's
    /// luminance over black. Both are soft, so that what was already clear of
    /// them barely moves and nothing that was in order turns over.
    pub fn enforce(&self, img: &mut Image) -> (usize, usize) {
        let (mut capped, mut lifted) = (0, 0);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            let (h, s) = (self.held[i] as f32 / 255.0, self.seen[i] as f32 / 255.0);
            let y = kit::luminance_px(px);
            if s > 0.0 && y < 3.0 * SEEN_FLOOR_Y {
                // (Y⁴ + floor⁴)^¼: the floor at black, the pixel itself well
                // above it. The light it lacks is added to every channel
                // alike, rather than the pixel scaled: that close to black its
                // colour is mostly the sensor's noise, which scaling would
                // bring up with it, magenta and all.
                let m = s * SEEN_FLOOR_Y;
                let target = (y.max(0.0).powi(4) + m.powi(4)).powf(0.25);
                let add = target - y.max(0.0);
                *px = px.map(|v| v.max(0.0) + add);
                lifted += 1;
            }
            let top = px[0].max(px[1]).max(px[2]);
            if h > 0.0 && top > HELD_TOP_RGB - 0.06 {
                let (mut l, mut a, mut b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
                // The same on the distance from white, in lightness.
                let m = h * (1.0 - HELD_TOP_RGB.cbrt());
                let x = (1.0 - l).max(0.0);
                l = 1.0 - (x.powi(4) + m.powi(4)).powf(0.25);
                // And the colour drawn in until every channel fits under the
                // ceiling, keeping its hue and lightness.
                let limit = 1.0 - h * (1.0 - HELD_TOP_RGB);
                let fits = |k: f32| {
                    let (r, g, bl) = kit::oklab_to_rgb_px(l, a * k, b * k);
                    r.max(g).max(bl) <= limit
                };
                if !fits(1.0) {
                    let (mut lo, mut hi) = (0.0f32, 1.0f32);
                    for _ in 0..12 {
                        let mid = 0.5 * (lo + hi);
                        if fits(mid) { lo = mid } else { hi = mid }
                    }
                    a *= lo;
                    b *= lo;
                }
                let (r, g, bl) = kit::oklab_to_rgb_px(l, a, b);
                *px = [r.clamp(0.0, limit), g.clamp(0.0, limit), bl.clamp(0.0, limit)];
                capped += 1;
            }
        }
        (capped, lifted)
    }
}

/// The brightest a held pixel's channels may come out, display-linear: code
/// 253 of 255.
const HELD_TOP_RGB: f32 = 0.98;
/// The darkest a seen pixel's luminance may come out, display-linear: code 10
/// of 255, clear of the 8 that counts as crushed.
const SEEN_FLOOR_Y: f32 = 0.003;

/// Where `filmic` brings a held highlight's brightest channel at most, and how
/// far below that the roll-off toward it starts, in stops.
const HELD_TOP_DISPLAY: f32 = 0.9;
const HELD_KNEE_STOPS: f32 = 1.5;

/// A roll-off, in log2 of the brightest channel, toward the scene level
/// `filmic` shows at `HELD_TOP_DISPLAY`: untouched below the knee, smooth
/// through it, and never reaching the top.
struct HeldCeiling {
    knee: f32,
    room: f32,
}

impl HeldCeiling {
    fn new() -> HeldCeiling {
        // The scene level the curve takes to the top: the positive root of
        // (t C - A) x² + (t D - B) x + t E = 0.
        let [a, b, c, d, e] = FILMIC;
        let t = HELD_TOP_DISPLAY;
        let (qa, qb, qc) = (t * c - a, t * d - b, t * e);
        let x = (-qb - (qb * qb - 4.0 * qa * qc).sqrt()) / (2.0 * qa);
        let top = x.log2();
        HeldCeiling { knee: top - HELD_KNEE_STOPS, room: HELD_KNEE_STOPS }
    }

    fn apply(&self, level: f32) -> f32 {
        if level <= self.knee {
            level
        } else {
            self.knee + self.room * ((level - self.knee) / self.room).tanh()
        }
    }
}

/// Scene-linear -> display-linear with a filmic shoulder.
pub fn filmic(img: &mut Image) {
    for px in img.d.chunks_exact_mut(3) {
        // Scaled by the brightest channel, not by luminance: a saturated colour
        // has little luminance for its brightest channel, so a ratio taken from
        // luminance sends that channel past white, where it clips flat.
        let norm = px[0].max(px[1]).max(px[2]).max(EPS);
        let ty = curve(norm);
        let w = kit::smoothstep(0.55, 1.0, ty);
        let hue_preserving = [px[0] * (ty / norm), px[1] * (ty / norm), px[2] * (ty / norm)];
        let mut out = hue_preserving;
        if w > 0.0 {
            for c in 0..3 {
                out[c] = hue_preserving[c] * (1.0 - w) + curve(px[c]) * w;
            }
            // Keep the per-channel curve's lightness and chroma (it bleaches
            // bright colours toward white) but the hue-preserving curve's hue.
            let (_, a0, b0) = kit::rgb_to_oklab_px(hue_preserving[0], hue_preserving[1],
                                                   hue_preserving[2]);
            let c0 = a0.hypot(b0);
            if c0 > 1e-4 {
                let (l, a, b) = kit::rgb_to_oklab_px(out[0], out[1], out[2]);
                let chroma = a.hypot(b);
                let at = |k: f32| {
                    let (r, g, bl) = kit::oklab_to_rgb_px(l, a0 / c0 * chroma * k,
                                                          b0 / c0 * chroma * k);
                    [r, g, bl]
                };
                let fits = |c: [f32; 3]| c.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v));
                out = at(1.0);
                // Turned back to the hue it should have, a colour near white
                // can land outside the gamut: take its chroma down until it
                // fits, keeping its lightness and its hue.
                if !fits(out) {
                    let (mut lo, mut hi) = (0.0f32, 1.0f32);
                    for _ in 0..10 {
                        let mid = 0.5 * (lo + hi);
                        if fits(at(mid)) { lo = mid } else { hi = mid }
                    }
                    out = at(lo);
                }
            }
        }
        for c in 0..3 {
            // Nothing the curve makes is past white; below black stays loose,
            // for the gamut mapping at the end to deal with.
            px[c] = out[c].clamp(-1.0, 1.0);
        }
    }
}

// Perceptual stages (Oklab)
/// Steps in the contrast curve's per-render table.
const CURVE_STEPS: usize = 4096;

/// Smooth contrast curve anchored at the image's own midtone.
///
/// Midtone contrast only: the bend fades out toward black and white, so the
/// curve leaves both ends at their own slope instead of pressing shadows into
/// black and highlights into white.
pub fn s_curve(x: f32, amount: f32, pivot: f32) -> f32 {
    if amount <= 1e-3 {
        return x;
    }
    let g = (0.5f32).ln() / pivot.clamp(0.1, 0.9).ln();
    let t = x.clamp(0.0, 1.0).powf(g);
    let bend = (kit::smoothstep(0.0, 1.0, t) - t) * 4.0 * t * (1.0 - t);
    let y = (t + amount * bend).clamp(0.0, 1.0).powf(1.0 / g);
    // The bend fades toward black in `t`, but a low pivot stretches the
    // shadows out in `t`, and a deep shadow could lose a stop and a half. On
    // the dark side, the darkening also fades out in lightness itself.
    if y < x { x + (y - x) * kit::smoothstep(0.0, SHADOW_FADE, x) } else { y }
}

/// The lightness below which the contrast curve's darkening fades out.
const SHADOW_FADE: f32 = 0.25;

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
                        focus_sigma: f32, render_scale: f32,
                        long_edge: usize, d: &mut Decisions, frozen: bool,
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

    // Black/white point.
    let levels = if frozen {
        d.levels
    } else {
        let mut peak = Plane::new(img.w, img.h);
        for (v, px) in peak.d.iter_mut().zip(img.px()) {
            *v = px[0].max(px[1]).max(px[2]).max(0.0).cbrt();
        }
        let (black, mut white, mut info) =
            analyze::auto_levels(&measured(&lightness, stats_rect),
                                 &measured(&peak, stats_rect), p.levels_strength, 0.15);
        // A merged bracket was shot so that nothing at the top is lost: a
        // white point below one would push its brightest percentile, which
        // the dark frames held, to white. Measured on the preview, it would
        // push still more of the export there, whose peaks are sharper.
        if p.keep_highlights && white < 1.0 {
            white = 1.0;
            info = with(info, json!({"white": 1.0, "white_held": true}));
        }
        let levels_applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
        if s.on("levels") && levels_applied {
            report.add("levels", info);
            Some((black, white))
        } else {
            report.skipped("levels", if !s.on("levels") { "switched_off" } else { "not_needed" },
                           info);
            None
        }
    };
    d.levels = levels;
    let mut light_new = Plane::new(img.w, img.h);
    if let Some((black, white)) = levels {
        for i in 0..n {
            light_new.d[i] = analyze::levels(lightness.d[i], black, white);
        }
    } else {
        light_new.d.copy_from_slice(&lightness.d);
    }

    // Contrast.
    let (amount, pivot) = if frozen {
        d.contrast
    } else {
        let (amount, pivot, info) = analyze::auto_contrast(&measured(&light_new, stats_rect), 0.20,
                                                           p.contrast_amount);
        if s.on("contrast") {
            report.add("contrast", info);
        } else {
            report.skipped("contrast", "switched_off", info);
        }
        (amount, pivot)
    };
    d.contrast = (amount, pivot);
    if s.on("contrast") {
        // Tabulated: the curve costs two `powf` per evaluation.
        let table: Vec<f32> = (0..=CURVE_STEPS + 1)
            .map(|i| s_curve((i as f32 / CURVE_STEPS as f32).min(1.0), amount, pivot))
            .collect();
        for (v, px) in light_new.d.iter_mut().zip(img.px()) {
            let f = v.clamp(0.0, 1.0) * CURVE_STEPS as f32;
            let j = f as usize;
            let curved = table[j] + (table[j + 1] - table[j]) * (f - j as f32);
            // Brightening a colour whose brightest channel is already near the
            // top would take it out of the gamut, where the mapping at the end
            // presses it flat: a bright saturated gradient would lose its
            // steps. The lift fades out there; darkening is always kept.
            let room = 1.0 - kit::smoothstep(0.85, 1.0, px[0].max(px[1]).max(px[2]));
            *v = if curved > *v { *v + (curved - *v) * room } else { curved };
        }
    }

    // Tone zones.
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

    // Scale chroma with the lightness change, or lifted shadows go grey. Partly
    // on the way up, so a lift does not oversaturate; fully on the way down,
    // or a darkened region keeps its chroma and reads more saturated (and any
    // tint in it stronger) than before.
    for i in 0..n {
        let ratio = (light_new.d[i].max(0.0) + 1e-3) / (lightness.d[i].max(0.0) + 1e-3);
        let exponent = if ratio < 1.0 { 1.0 } else { 0.35 };
        let comp = kit::fast_exp2(exponent * kit::fast_log2(ratio)).clamp(0.25, 1.8);
        a.d[i] *= comp;
        b.d[i] *= comp;
    }
    drop(lightness);

    // Colour noise. The slider moves the measured amount rather than
    // replacing it.
    let auto_chroma = if p.denoise { ((noise - 0.18) / 0.5).clamp(0.0, 1.0) * 0.85 } else { 0.0 };
    let chroma_blend = with_user(auto_chroma, s.denoise, 0.95);
    if s.on("denoise_chroma") && chroma_blend > 0.01 {
        let radius = ((long_edge as f32 / 350.0).round() as usize).max(3);
        let blend = chroma_blend;
        let [fa, fb] = kit::guided_filter_each(&light_new, [&a, &b], radius, 4e-4, 2);
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

    // Luminance noise. The slider may go further than the automation does
    // on its own (0.85 against 0.55): only a person can judge the texture lost.
    let auto_luma = if p.denoise { ((noise - 0.30) / 0.6).clamp(0.0, 1.0) * 0.55 } else { 0.0 };
    let luma_blend = with_user(auto_luma, s.denoise, 0.85);
    if s.on("denoise_luma") && luma_blend > 0.01 {
        let radius = ((long_edge as f32 / 900.0).round() as usize).max(2);
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

    // Vibrance.
    let mut chroma = Plane::new(img.w, img.h);
    for i in 0..n {
        chroma.d[i] = a.d[i].hypot(b.d[i]);
    }
    let boost = if frozen { d.vibrance } else {
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
    boost
    };
    d.vibrance = boost;
    // The look's own saturation, on every colour; switched off with vibrance.
    let saturation = if s.on("vibrance") { p.saturation } else { 1.0 };
    {
        for i in 0..n {
            let c = chroma.d[i];
            let hue = kit::fast_atan2(b.d[i], a.d[i]);
            // Vibrance lifts what is muted and leaves what is already vivid.
            let muted = 1.0 - kit::smoothstep(0.10, 0.30, c);
            // Neither touches what is meant to be neutral (its colour is a
            // white balance error), nor what is nearly white, where there is
            // no room left in the gamut, nor skin, as far as it is protected.
            let room = kit::smoothstep(0.5 * analyze::NEUTRAL_CHROMA,
                                       2.0 * analyze::NEUTRAL_CHROMA, c)
                * (1.0 - kit::smoothstep(0.85, 0.97, light_new.d[i]))
                * (1.0 - s.skin_protect * s.skin.likeness(hue, c));
            // The curve's own depth, before levels and contrast moved the
            // colour, on every colour.
            let depth = filmic_depth(img.px()[i]);
            let mut gain = (1.0 + ((boost - 1.0) * muted + (saturation - 1.0)) * room) * depth;
            // Never to the gamut's edge: the mapping at the end would press the
            // colour onto it, and a bright saturated gradient would come out
            // flat there. Only as far as leaves a little room, and not at all
            // for a colour already that close.
            if gain > 1.0 {
                let l = light_new.d[i];
                let fits = |g: f32| {
                    let (r, gg, bl) = kit::oklab_to_rgb_px(l, a.d[i] * g, b.d[i] * g);
                    r.min(gg).min(bl) >= 0.0 && r.max(gg).max(bl) <= BOOST_HEADROOM
                };
                if !fits(gain) {
                    let (mut lo, mut hi) = (1.0f32, gain);
                    if !fits(lo) {
                        hi = lo;
                    }
                    for _ in 0..8 {
                        let mid = 0.5 * (lo + hi);
                        if fits(mid) { lo = mid } else { hi = mid }
                    }
                    gain = lo;
                }
            }
            a.d[i] *= gain;
            b.d[i] *= gain;
        }
    }
    drop(chroma);

    // Focus recovery, before sharpening.
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

    // Sharpening.
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

// Entry point
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
    /// Where this image sits in the frame; the whole of it, except for a
    /// piece developed on its own (`Development::render_region`).
    pub window: Window,
    /// Decisions to apply rather than make; see `Decisions`.
    pub decided: Option<Decisions>,
    /// The sky and subject masks the filters may follow, over the framed picture.
    pub masks: &'a crate::masks::Masks,
    /// For a merged bracket, `Vouched::marks` taken before the lens and the
    /// framing and put through them with the frame; measured on the frame
    /// itself when none are given.
    pub vouched: Option<Vouched>,
    pub progress: Option<&'a mut dyn FnMut(f32, &str)>,
}

/// Linear scene-referred RGB -> display-referred linear sRGB in [0,1], with
/// what was decided along the way.
pub fn process(mut img: Image, settings: &Settings, args: ProcessArgs, report: &mut Report)
               -> (Image, Decisions) {
    let p = args.preset;
    let frozen = args.decided.is_some();
    let mut d = args.decided.unwrap_or_default();
    let long_edge = args.window.long_edge();
    let frame = args.stats_rect.or(args.window.frame(&img));
    // Healing before anything is measured; see `local`.
    crate::local::heal(&mut img, &settings.spots, frame);
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
    let vouched = match args.vouched {
        Some(v) => Some(v),
        None => Vouched::measure(&img, &p),
    };

    step(&mut progress, &mut mark);
    // One thumbnail for every scene-linear measurement, carried through the
    // same white balance and exposure as the frame (see apply_white_balance).
    let mut thumb = if frozen { Image::new(1, 1) } else { measure(&img, args.stats_rect) };
    apply_white_balance(&mut img, &mut thumb, settings, &p, args.stats_rect, &mut d, frozen,
                        report);
    step(&mut progress, &mut mark);
    let exposure_gain = apply_exposure(&mut img, &thumb, settings, &p, args.stats_rect, &mut d,
                                       frozen, report);
    thumb.scale(exposure_gain);
    step(&mut progress, &mut mark);

    // Noise must be measured at native resolution.
    let (noise, info) = analyze::noise_from_floor(
        args.native_noise_floor * exposure_gain.max(1e-6).sqrt(), args.iso, args.noise_prior);
    report.add("noise", info);

    apply_dehaze(&mut img, &mut thumb, settings, long_edge, &mut d, frozen, report);

    step(&mut progress, &mut mark);
    apply_local_tone_map(&mut img, &thumb, settings, &p, long_edge, &mut d, frozen,
                         vouched.as_ref().map(|v| v.held.as_slice()), report);
    // Local filters on the light as it will be shown, before the shoulder
    // rolls it off; see `local` for why not earlier.
    crate::local::apply_filters(&mut img, &settings.filters, frame, args.masks);
    report.add("filters", json!({"count": settings.filters.len()}));
    step(&mut progress, &mut mark);
    filmic(&mut img);
    step(&mut progress, &mut mark);
    let mut out = apply_perceptual(&img, settings, &p, noise, args.sharpen_sigma, args.focus_sigma, args.render_scale, long_edge, &mut d, frozen,
                               report, args.stats_rect);
    if let Some(v) = &vouched {
        let (capped, lifted) = v.enforce(&mut out);
        report.add("bracket bounds", json!({"capped": capped, "lifted": lifted}));
    }
    step(&mut progress, &mut mark);
    (out, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A merged bracket's shoulder leaves the shadows and midtones where the
    /// exposure put them and brings the highlights down to its top, smoothly.
    /// The bounds a merged bracket's frames set: a held pixel is brought
    /// under white, a seen one over black, and a pixel clear of both, or one
    /// no frame vouched for, is left exactly as it was.
    #[test]
    fn what_the_frames_vouched_for_is_kept_and_nothing_else_moves() {
        let mut img = Image::new(4, 1);
        img.px_mut().copy_from_slice(&[[1.0, 0.9, 0.2], [0.0002, 0.0001, 0.0003],
                                       [0.3, 0.2, 0.1], [1.0, 1.0, 1.0]]);
        let before = img.clone();
        let v = Vouched { w: 4, held: vec![255, 0, 255, 0], seen: vec![0, 255, 255, 0] };
        let (capped, lifted) = v.enforce(&mut img);
        assert_eq!((capped, lifted), (1, 1));
        let [held, seen, clear, unvouched] = [0, 1, 2, 3].map(|i| img.px()[i]);
        assert!(held.iter().all(|c| *c <= HELD_TOP_RGB + 1e-4), "{held:?}");
        assert!(held[0] > held[1] && held[1] > held[2], "the colour kept its order: {held:?}");
        assert!(kit::luminance_px(&seen) >= SEEN_FLOOR_Y * 0.999, "{seen:?}");
        let spread = |p: [f32; 3]| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]);
        assert!((spread(seen) - spread(before.px()[1])).abs() < 1e-6,
                "the noise was lifted, not amplified: {seen:?}");
        assert_eq!(clear, before.px()[2]);
        assert_eq!(unvouched, before.px()[3], "blown in every frame, white it may stay");
        let piece = v.crop(1, 0, 2, 1);
        assert_eq!((piece.held, piece.seen), (vec![0, 255], vec![255, 255]));
    }

    /// The roll-off toward a held highlight's ceiling leaves what is below its
    /// knee alone, bends without a kink, and never arrives.
    #[test]
    fn the_held_ceiling_bends_without_a_kink_and_never_arrives() {
        let c = HeldCeiling::new();
        assert_eq!(c.apply(c.knee - 1.0), c.knee - 1.0);
        let slope = (c.apply(c.knee + 1e-3) - c.knee) / 1e-3;
        assert!((slope - 1.0).abs() < 0.01, "a kink at the knee: {slope}");
        let top = c.knee + c.room;
        assert!(c.apply(top + 2.0) < top - 0.01, "short of the top a little over it");
        assert!(c.apply(top + 20.0) <= top, "and never past it");
        assert!((curve(top.exp2()) - HELD_TOP_DISPLAY).abs() < 1e-3);
    }

    #[test]
    fn the_shoulder_fits_the_highlights_and_leaves_the_rest() {
        let s = Shoulder::fitting(2.0).expect("two stops over white needs one");
        for level in [-6.0, -3.0, SHOULDER_KNEE.log2()] {
            assert_eq!(s.apply(level), level, "{level} was moved");
        }
        assert!((s.apply(2.0) - SHOULDER_TOP.log2()).abs() < 1e-4);
        let mut last = f32::MIN;
        for i in 0..200 {
            let out = s.apply(-4.0 + i as f32 * 0.05);
            assert!(out > last, "not rising at {i}");
            last = out;
        }
        let slope = (s.apply(SHOULDER_KNEE.log2() + 1e-3) - SHOULDER_KNEE.log2()) / 1e-3;
        assert!((slope - 1.0).abs() < 0.01, "a kink at the knee: {slope}");
        assert!(Shoulder::fitting(SHOULDER_TOP.log2() - 0.1).is_none(), "nothing to fit");
    }

    /// Unmeasured skin keeps the broad default target, measured skin is narrower,
    /// and a frame with nobody in it has none.
    #[test]
    fn skin_is_protected_as_measured() {
        let typical = Skin::default();
        let old = |hue: f32, chroma: f32| {
            let mut d = (hue - typical.hue).abs();
            while d > std::f32::consts::PI {
                d = (2.0 * std::f32::consts::PI - d).abs();
            }
            (1.0 - kit::smoothstep(0.25, 0.60, d)) * (1.0 - kit::smoothstep(0.10, 0.20, chroma))
        };
        for (hue, chroma) in [(0.9, 0.05), (1.3, 0.12), (-2.0, 0.08), (0.5, 0.18)] {
            assert_eq!(typical.likeness(hue, chroma), old(hue, chroma));
        }
        let measured = Skin::from_json(Some(&json!({"hue": 0.8, "chroma": 0.07})));
        assert_eq!(measured.source, SkinSource::Measured);
        assert!(measured.likeness(0.8, 0.07) > 0.99);
        assert!(measured.likeness(1.3, 0.07) < 0.05, "a yellow taken for skin");
        assert!(measured.likeness(0.8, 0.005) < 0.05, "a grey taken for skin");
        // Not looked for yet reads as the typical skin; looked for and not
        // found protects nothing, however skin-like the colour.
        assert_eq!(Skin::from_json(None), typical);
        assert_eq!(Skin::from_json(Some(&json!(false))), typical);
        let nobody = Skin::from_json(Some(&json!({"faces": 0})));
        assert_eq!(nobody.source, SkinSource::NoFaces);
        assert_eq!(nobody.likeness(typical.hue, 0.05), 0.0);
    }

    /// The interface draws the white balance and exposure boxes on the framed
    /// picture.
    #[test]
    fn a_box_is_read_inside_the_kept_frame() {
        let mut img = Image::new(8, 8);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            *px = [(i % 8) as f32 / 10.0, (i / 8) as f32 / 10.0, 0.5];
        }
        // The frame is the middle half; its top-left quarter is canvas 2..4.
        let got = sample_rect(&img, (0.0, 0.0, 0.5, 0.5), Some((0.25, 0.25, 0.5, 0.5))).unwrap();
        let mut want = Vec::new();
        for y in 2..4 {
            for x in 2..4 {
                want.push(img.px()[y * 8 + x]);
            }
        }
        assert_eq!(got, want);
        // With no frame the box is in canvas fractions.
        let whole = sample_rect(&img, (0.0, 0.0, 0.25, 0.25), None).unwrap();
        assert_eq!(whole, vec![img.px()[0], img.px()[1], img.px()[8], img.px()[9]]);
    }

    /// The shadows are lifted by what they need: never by more because the
    /// sum came out a hair short, which jumped the pivot to the highlights.
    #[test]
    fn the_shadows_are_lifted_by_what_they_need() {
        for i in 0..40 {
            for j in 0..40 {
                let (low, high) = (-9.0 + i as f32 * 0.07, -0.5 + j as f32 * 0.011);
                for factor in [0.55f32, 0.62, 0.7, 0.803, 0.9] {
                    let need = (analyze::DISPLAY_FLOOR.log2() - low).clamp(0.0, analyze::MAX_RESCUE_EV);
                    let (anchor, f) = lift_shadows(low, low + 0.3, high, factor, need);
                    let lift = (anchor - low) * (1.0 - f);
                    assert!(lift <= need + 1e-3 || f <= TONE_FLOOR + 1e-6,
                            "low {low} high {high} factor {factor}: lifted {lift} for {need}");
                    assert!(lift >= need - 1e-3 || f <= TONE_FLOOR + 1e-6,
                            "low {low} high {high} factor {factor}: lifted {lift} short of {need}");
                }
            }
        }
    }

    /// A low pivot stretches the shadows out under the curve; a deep shadow
    /// must still keep most of its light, not lose a stop and a half to it.
    #[test]
    fn the_curve_does_not_bury_a_deep_shadow() {
        let out = s_curve(0.08, 0.65, 0.3);
        let stops = (out / 0.08).powi(3).log2();
        assert!(stops > -0.4, "a shadow at lightness 0.08 went to {out}, {stops} stops down");
    }

    /// The S-curve is pivoted on the image's own midtone, so it must hold the
    /// pivot in place while it steepens either side of it.
    #[test]
    fn s_curve_matches_reference() {
        for (x, want) in [(0.1f32, 0.095255f32), (0.25, 0.223126), (0.45, 0.45),
                          (0.75, 0.779422), (0.9, 0.90955)] {
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
        // A pixel blown in every channel stays white.
        let mut blown = Image::filled(1, 1, [1.0; 3]);
        adapt(&mut blown, gains);
        let spread = blown.d.iter().cloned().fold(f32::MIN, f32::max)
            - blown.d.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 1e-5, "a blown pixel was tinted: {:?}", blown.d);
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

    /// Zero must be exactly untouched: every frame goes through this path.
    #[test]
    fn zero_zones_are_the_identity() {
        for i in 0..=100 {
            let l = i as f32 / 100.0;
            assert_eq!(apply_zones(l, 0.0, 0.0, 0.0), l);
        }
    }

    /// Each slider owns its end of the range and leaves the other alone.
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
            apply_local_tone_map(&mut img, &src.clone(), &s, &Preset::default(), src.w.max(src.h),
                                 &mut Decisions::default(), false, None, &mut report);
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
            apply_perceptual(&src, &s, &Preset::default(), noise, 1.0, 0.0, 1.0,
                             src.w.max(src.h), &mut Decisions::default(), false, &mut report, None);
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

//! What the camera already told us about the shot.

use crate::decode::Meta;
use crate::grade::Preset;
use kit::round_to;
use serde_json::{json, Value};

pub const LAMBDA_UM: f32 = 0.55;
pub const FULL_FRAME_DIAGONAL_MM: f32 = 43.27;

#[derive(Clone, Debug)]
pub struct CaptureProfile {
    pub camera: String,
    pub lens: String,
    pub iso: f32,
    pub aperture: f32,
    pub focal_mm: f32,
    pub focal35_mm: f32,
    pub shutter_s: f32,
    pub exposure_comp: f32,
    pub flash: bool,
    pub crop_factor: f32,
    pub pixel_pitch_um: f32,
    pub megapixels: f32,
    // derived
    pub airy_um: f32,
    pub diffraction_ratio: f32,
    pub sharpen_sigma: f32,
    pub noise_prior: f32,
    pub usable_stops: f32,
    pub shake_risk: f32,
    pub tele_bias: f32,
    /// How bright the scene was, as EV at ISO 100, when the file says enough
    /// to tell: daylight is 13 to 16, a lit room 5 to 7, a city at night 0 to 3.
    pub scene_ev: Option<f32>,
    /// `{code, params}`, not prose: the wording is the interface's business.
    pub notes: Vec<Value>,
}

/// A smooth 0 to 1 ramp between two edges.
fn smooth(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}


impl CaptureProfile {
    /// Raw numbers for a client to format in whatever locale it is in.
    pub fn values(&self) -> Value {
        json!({
            "camera": self.camera,
            "lens": self.lens,
            "iso": self.iso as i64,
            "aperture": round_to(self.aperture, 2),
            "focal_mm": round_to(self.focal_mm, 1),
            "focal35_mm": round_to(self.focal35_mm, 1),
            "shutter_s": round_to(self.shutter_s, 6),
            "megapixels": round_to(self.megapixels, 1),
            "pixel_pitch_um": round_to(self.pixel_pitch_um, 2),
            "crop_factor": round_to(self.crop_factor, 2),
            "airy_um": round_to(self.airy_um, 1),
            "diffraction_ratio": round_to(self.diffraction_ratio, 1),
            "sharpen_sigma": round_to(self.sharpen_sigma, 2),
            "noise_prior": round_to(self.noise_prior, 3),
            "usable_stops": round_to(self.usable_stops, 1),
            "exposure_comp": round_to(self.exposure_comp, 1),
            "scene_ev": self.scene_ev.map(|v| round_to(v, 1)),
        })
    }
}

/// Build the capture profile from EXIF plus the decoded image dimensions.
pub fn derive(meta: &Meta, w: usize, h: usize, crop_factor: Option<f32>) -> CaptureProfile {
    let mut p = CaptureProfile {
        camera: if meta.model.is_empty() { "unknown".into() } else { meta.model.clone() },
        lens: meta.lens_model.clone(),
        iso: if meta.iso > 0.0 { meta.iso } else { 100.0 },
        aperture: if meta.aperture > 0.0 { meta.aperture } else { 8.0 },
        focal_mm: if meta.focal_mm > 0.0 { meta.focal_mm } else { 50.0 },
        focal35_mm: 50.0,
        shutter_s: if meta.shutter_s > 0.0 { meta.shutter_s } else { 1.0 / 125.0 },
        exposure_comp: meta.exposure_comp,
        flash: meta.flash,
        crop_factor: 1.5,
        pixel_pitch_um: 4.0,
        megapixels: (w * h) as f32 / 1e6,
        airy_um: 0.0,
        diffraction_ratio: 1.0,
        sharpen_sigma: 1.0,
        noise_prior: 0.0,
        usable_stops: 12.0,
        shake_risk: 0.0,
        tele_bias: 0.0,
        scene_ev: None,
        notes: Vec::new(),
    };
    if meta.shutter_s > 0.0 && meta.aperture > 0.0 && meta.iso > 0.0 {
        p.scene_ev = Some((meta.aperture * meta.aperture / meta.shutter_s).log2()
                          - (meta.iso / 100.0).log2());
    }

    // Crop factor: the lens database's body entry first, then the ratio EXIF
    // gives between real and 35mm-equivalent focal length.
    if let Some(cf) = crop_factor {
        if cf > 0.2 {
            p.crop_factor = cf;
        }
    } else if meta.focal35_mm > 0.0 && p.focal_mm > 0.0 {
        p.crop_factor = meta.focal35_mm / p.focal_mm;
    }
    p.focal35_mm = p.focal_mm * p.crop_factor;

    // Sensor geometry -> pixel pitch.
    let diag_mm = FULL_FRAME_DIAGONAL_MM / p.crop_factor.max(0.2);
    let sensor_w_mm = diag_mm * w as f32 / ((w * w + h * h) as f32).sqrt();
    p.pixel_pitch_um = sensor_w_mm * 1000.0 / (w.max(1) as f32);

    // --- diffraction ------------------------------------------------------
    p.airy_um = 2.44 * LAMBDA_UM * p.aperture;
    p.diffraction_ratio = p.airy_um / p.pixel_pitch_um.max(0.1);
    // An Airy disc is well approximated by a Gaussian of sigma ~ 0.42*lambda*N;
    // the pixel aperture and demosaic add in quadrature, hence the 0.7 px floor.
    let diffraction_sigma = 0.42 * LAMBDA_UM * p.aperture / p.pixel_pitch_um.max(0.1);
    p.sharpen_sigma = diffraction_sigma.hypot(0.70).clamp(0.7, 2.6);
    if p.diffraction_ratio > 2.0 {
        p.notes.push(json!({"code": "diffraction", "params": {
            "aperture": p.aperture, "pitch": round_to(p.pixel_pitch_um, 2),
            "ratio": round_to(p.diffraction_ratio, 1), "sigma": round_to(p.sharpen_sigma, 2)}}));
    }

    // --- noise ------------------------------------------------------------
    // Doubling ISO costs a stop; doubling pixel area buys one back.
    let penalty = (p.iso.max(25.0) / 100.0).log2() - 2.0 * (p.pixel_pitch_um.max(0.5) / 4.0).log2();
    p.noise_prior = (penalty / 7.0).clamp(0.0, 1.0);
    p.usable_stops = (12.6 - penalty.max(0.0) * 0.85).clamp(5.5, 13.5);
    if p.noise_prior > 0.25 {
        p.notes.push(json!({"code": "noise",
            "params": {"iso": p.iso as i64, "stops": round_to(p.usable_stops, 1)}}));
    }

    // --- camera shake -----------------------------------------------------
    // The hand-holding rule only describes a hand.
    if p.shutter_s > 0.0 && p.focal35_mm > 0.0 {
        let handheld = 1.0 - smooth(0.5, 2.0, p.shutter_s);
        p.shake_risk =
            ((p.shutter_s * p.focal35_mm).log2() / 2.0).clamp(0.0, 1.0) * handheld;
        if p.shake_risk > 0.3 {
            p.notes.push(json!({"code": "shake", "params": {
                "shutter_s": p.shutter_s, "focal35": p.focal35_mm.round() as i64}}));
        }
    }

    // --- what kind of picture is this ------------------------------------
    p.tele_bias = ((p.focal35_mm.max(8.0) / 45.0).log2() / 2.0).clamp(-1.0, 1.0);

    if p.exposure_comp.abs() > 0.15 {
        p.notes.push(json!({"code": "exposure_comp", "params": {"ev": round_to(p.exposure_comp, 1)}}));
    }
    p
}

/// Return a copy of `preset` adjusted by the capture profile.
pub fn tune(preset: &Preset, p: &CaptureProfile) -> Preset {
    let mut out = preset.clone();

    // Deliberate under/over-exposure is intent; keep half of it.
    let mut target_key = preset.target_key * (p.exposure_comp * 0.5).exp2();
    // A high-ISO frame is usually a dark scene: pushing it to a daylight
    // midtone both lies about the light and drags up the noise floor.
    target_key *= (1.0 - 0.30 * p.noise_prior).clamp(0.6, 1.0);
    out.target_key = target_key.clamp(0.04, 0.30);

    // Never ask for more shadow lift than the sensor can deliver cleanly.
    out.comfortable_stops = preset.comfortable_stops.min(p.usable_stops - 1.5).clamp(4.5, 9.0);

    // Long lens: a subject, so protect colour and go easy on local contrast.
    let detail = preset.detail_boost * (1.0 - 0.18 * p.tele_bias) * (1.0 - 0.35 * p.noise_prior);
    out.detail_boost = detail.clamp(1.0, 1.35);
    out.target_chroma = (preset.target_chroma * (1.0 - 0.10 * p.tele_bias)).clamp(0.05, 0.12);

    let mut sharpen = preset.sharpen;
    sharpen *= 1.0 + 0.25 * (p.diffraction_ratio - 2.0).clamp(0.0, 2.0); // deterministic blur
    sharpen *= 1.0 - 0.55 * p.shake_risk; // blur we cannot fix
    sharpen *= 1.0 - 0.45 * p.noise_prior;
    out.sharpen = sharpen.clamp(0.0, 1.2);

    // The automatic white balance assumes the scene averages out to grey, which
    // daylight mostly does and a city at night does not.
    if let (Some(ev), false) = (p.scene_ev, p.flash) {
        out.wb_strength = preset.wb_strength * (0.3 + 0.7 * smooth(2.0, 7.0, ev));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Meta;

    fn shot(shutter_s: f32, focal_mm: f32) -> Meta {
        Meta { shutter_s, focal_mm, iso: 200.0, aperture: 5.6, ..Default::default() }
    }

    /// Nobody hand-holds a thirty second exposure, so it must not be treated
    /// as a shaky one and stripped of its sharpening.
    #[test]
    fn a_long_exposure_is_a_supported_one() {
        let handheld = derive(&shot(1.0 / 8.0, 200.0), 6000, 4000, Some(1.5));
        let tripod = derive(&shot(30.0, 50.0), 6000, 4000, Some(1.5));
        assert!(handheld.shake_risk > 0.5,
                "an eighth of a second at 200mm is a shaky frame: {}", handheld.shake_risk);
        assert_eq!(tripod.shake_risk, 0.0,
                   "a thirty second frame is on a tripod, not a hand");
    }

    /// The taper has to be gradual: a second and a half is genuinely ambiguous.
    #[test]
    fn the_handheld_assumption_fades_rather_than_switches() {
        let quick = derive(&shot(0.4, 50.0), 6000, 4000, Some(1.5)).shake_risk;
        let middling = derive(&shot(1.2, 50.0), 6000, 4000, Some(1.5)).shake_risk;
        let long = derive(&shot(3.0, 50.0), 6000, 4000, Some(1.5)).shake_risk;
        assert!(quick > middling && middling > long, "{quick} {middling} {long}");
        assert_eq!(long, 0.0);
    }

    /// Daylight keeps the whole automatic balance.
    #[test]
    fn the_automatic_balance_fades_with_the_light() {
        let preset = Preset::default();
        let at = |iso: f32, aperture: f32, shutter_s: f32, flash: bool| {
            let meta = Meta { iso, aperture, shutter_s, flash, focal_mm: 35.0, ..Default::default() };
            tune(&preset, &derive(&meta, 6000, 4000, Some(1.5))).wb_strength
        };
        let noon = at(100.0, 4.0, 1.0 / 800.0, false);
        let night = at(6400.0, 2.8, 0.1, false);
        let flash = at(6400.0, 2.8, 0.1, true);
        assert_eq!(noon, preset.wb_strength);
        assert!(night < 0.35 * preset.wb_strength, "night kept {night}");
        assert_eq!(flash, preset.wb_strength);
        let unknown = tune(&preset, &derive(&Meta::default(), 6000, 4000, None)).wb_strength;
        assert_eq!(unknown, preset.wb_strength, "a file that does not say was held back");
    }
}

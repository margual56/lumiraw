//! One raw file, developed many ways.

use crate::analyze::Report;
use crate::decode::{self, Meta};
use crate::geometry::{self, Rect};
use crate::grade::{self, ProcessArgs, Settings, TOGGLES};
use crate::lensdb::{self, Database, LensMatch};
use crate::ops::{self, Image};
use crate::profile::{self, CaptureProfile};
use crate::straighten;
use crate::styles;
use serde_json::{json, Map, Value};

pub struct Development {
    pub filename: String,
    pub linear: Image,
    pub meta: Meta,
    /// The original file's own metadata, kept so the export can carry it.
    pub exif: Vec<crate::exif::Entry>,
    /// The capture's noise floor, measured once from the full-resolution frame.
    noise_floor: Option<f32>,
    /// How wide this capture's edges are at `crate::analyze::FOCUS_LONG_EDGE`, and
    /// whether that could be measured at all.
    focus: Option<Option<f32>>,
    pub profile: CaptureProfile,
    pub lens_match: Option<LensMatch>,
    cache: Vec<(String, Image, Option<Rect>, Map<String, Value>)>,
    baseline_key: String,
    baseline_img: Option<Image>,
}

impl Development {
    pub fn open(filename: &str, bytes: &[u8], db: Option<&Database>) -> Result<Development, String> {
        let d = decode::decode(bytes)?;
        let (w, h) = (d.img.w, d.img.h);
        let mut crop_factor = None;
        let mut lens_match = None;
        if let Some(db) = db {
            if let Some(cam) = db.find_camera(&d.meta.make, &d.meta.model) {
                crop_factor = Some(cam.crop);
            }
            lens_match = db.find_lens(&d.meta.make, &d.meta.model, &d.meta.lens_model,
                                      d.meta.focal_mm, d.meta.aperture, 10.0);
        }
        let prof = profile::derive(&d.meta, w, h, crop_factor);
        Ok(Development {
            filename: filename.to_string(),
            linear: d.img,
            meta: d.meta,
            exif: decode::collect_exif(bytes),
            noise_floor: None,
            profile: prof,
            lens_match,
            cache: Vec::new(),
            focus: None,
            baseline_key: String::new(),
            baseline_img: None,
        })
    }

    /// The noise floor of this capture, at native resolution.
    fn native_noise_floor(&mut self) -> f32 {
        if let Some(floor) = self.noise_floor {
            return floor;
        }
        let crop = {
            let w = 640.min(self.linear.w);
            let h = 640.min(self.linear.h);
            self.linear.crop((self.linear.w - w) / 2, (self.linear.h - h) / 2, w, h)
        };
        let mut sqrt_luma = ops::luminance(&crop);
        for v in sqrt_luma.d.iter_mut() {
            *v = v.max(0.0).sqrt();
        }
        let floor = crate::analyze::noise_floor(&sqrt_luma);
        self.noise_floor = Some(floor);
        floor
    }

    /// How wide this frame's edges are, measured once from the whole picture.
    pub fn focus_width(&mut self) -> Option<f32> {
        if let Some(cached) = self.focus {
            return cached;
        }
        let measured = crate::analyze::focus_width(&self.linear, |p| crate::analyze::noise_floor(p));
        self.focus = Some(measured);
        measured
    }

    /// A development whose frame came from a merge rather than a single file.
    pub fn from_frame(filename: &str, linear: Image, meta: Meta,
                      exif: Vec<crate::exif::Entry>, db: Option<&Database>) -> Development {
        let (w, h) = (linear.w, linear.h);
        let mut crop_factor = None;
        let mut lens_match = None;
        if let Some(db) = db {
            if let Some(cam) = db.find_camera(&meta.make, &meta.model) {
                crop_factor = Some(cam.crop);
            }
            lens_match = db.find_lens(&meta.make, &meta.model, &meta.lens_model,
                                      meta.focal_mm, meta.aperture, 10.0);
        }
        let prof = profile::derive(&meta, w, h, crop_factor);
        Development {
            filename: filename.to_string(),
            linear,
            meta,
            exif,
            noise_floor: None,
            profile: prof,
            lens_match,
            cache: Vec::new(),
            focus: None,
            baseline_key: String::new(),
            baseline_img: None,
        }
    }

    pub fn width(&self) -> usize {
        self.linear.w
    }
    pub fn height(&self) -> usize {
        self.linear.h
    }

    /// Lens-corrected linear image at the requested size.
    fn corrected(&mut self, s: &Settings, long_edge: Option<usize>, crop: bool,
                 progress: &mut Option<&mut dyn FnMut(f32, &str)>)
                 -> (Image, Map<String, Value>, Option<Rect>) {
        let key = format!(
            "{:?}|{}|{}|{}|{}|{}",
            long_edge,
            s.on("lens_vignetting"),
            s.on("lens_distortion"),
            s.on("lens_tca"),
            s.framing.key(),
            crop
        );
        // The measurements travel with the cached proxy.
        if let Some((_, img, rect, infos)) = self.cache.iter().find(|(k, _, _, _)| *k == key) {
            return (img.clone(), infos.clone(), *rect);
        }
        let mut infos: Map<String, Value> = Map::new();

        let mut src = match long_edge {
            None => self.linear.clone(),
            Some(e) => ops::thumbnail(&self.linear, e),
        };
        if let Some(m) = self.lens_match.clone() {
            if s.on("lens_vignetting") {
                if let Some(cb) = progress.as_mut() {
                    cb(0.02, "vignetting");
                }
                let info = lensdb::apply_vignetting(&mut src, &m);
                infos.insert("vignetting".into(), info);
            }
            if s.on("lens_distortion") || s.on("lens_tca") {
                if let Some(cb) = progress.as_mut() {
                    cb(0.05, "geometry");
                }
                let (out, geo) = lensdb::apply_geometry(&src, &m, s.on("lens_distortion"),
                                                        s.on("lens_tca"));
                src = out;
                infos.insert("distortion".into(), geo.clone());
                infos.insert("chromatic aberration".into(), geo);
            }
        }
        // Framing comes after lens correction (distortion must be undone in the
        // original frame) and before everything tonal, so metering and the auto
        // levels only ever see the picture that will actually be kept.
        let eff = geometry::effective_crop(src.w, src.h, &s.framing);
        src = geometry::apply(&src, &s.framing, crop);
        if long_edge.is_some() {
            if self.cache.len() > 6 {
                self.cache.clear();
            }
            self.cache.push((key, src.clone(), eff, infos.clone()));
        }
        (src, infos, eff)
    }

    /// Develop the picture, optionally under a look.
    pub fn render(&mut self, settings: &Settings, style: Option<&styles::Style>,
                  long_edge: Option<usize>, crop: bool,
                  mut progress: Option<&mut dyn FnMut(f32, &str)>)
                  -> (Image, Report) {
        let mut report = Report::new();
        let (src, lens_info, eff_crop) = self.corrected(settings, long_edge, crop, &mut progress);
        self.report_lens(settings, &lens_info, &mut report);

        let scale = match long_edge {
            None => 1.0,
            Some(_) => src.w.max(src.h) as f32 / self.linear.w.max(self.linear.h) as f32,
        };
        let mut s = settings.clone();
        s.render_scale = scale;

        // When the canvas is shown uncropped, the estimators must still measure
        // only the rectangle being kept, see geometry::stats_view.
        let stats_rect = if crop { None } else { eff_crop };
        let tuned = profile::tune(&settings.preset_obj(), &self.profile);

        // Lens work is the first quarter of a render; grading is the rest.
        let mut graded_progress = progress.map(|cb| {
            let f: Box<dyn FnMut(f32, &str) + '_> =
                Box::new(move |f: f32, code: &str| cb(0.25 + 0.75 * f, code));
            f
        });
        let mut boxed: Option<&mut dyn FnMut(f32, &str)> = match graded_progress.as_mut() {
            Some(b) => Some(&mut **b),
            None => None,
        };

        let native_noise_floor = Some(self.native_noise_floor());
        // The width was measured at a fixed working size, so the blur it
        // describes is that many pixels *there*.
        let measured_width = self.focus_width();
        let focus_sigma = match measured_width {
            Some(w) if w >= crate::analyze::FOCUS_SOFT => {
                let at_work = crate::analyze::focus_sigma(w);
                let long = src.w.max(src.h) as f32;
                at_work * (long / crate::analyze::FOCUS_LONG_EDGE as f32).min(4.0)
            }
            _ => 0.0,
        };
        let args = ProcessArgs {
            iso: self.profile.iso,
            sharpen_sigma: self.profile.sharpen_sigma,
            noise_prior: self.profile.noise_prior,
            stats_rect,
            preset: Some(tuned),
            native_noise_floor,
            focus_sigma,
            progress: boxed.take(),
        };
        let mut rgb = grade::process(&src, &s, args, &mut report);

        // What the frame measured, said out loud whichever way it came out.
        report.add("focus", match measured_width {
            None => json!({"measured": false, "verdict": "unknown"}),
            Some(w) => json!({"measured": true,
                "verdict": if w >= crate::analyze::FOCUS_SOFT { "soft" }
                           else if w <= crate::analyze::FOCUS_SHARP { "sharp" }
                           else { "slightly_soft" },
                "edge_px": grade::round_to(w, 2),
                "blur_px": grade::round_to(crate::analyze::focus_sigma(w), 2)}),
        });

        // Framing is the one step that had nothing of its own to propose, so
        // the measurement rides along with the preview it was taken from,
        // against the graded frame rather than the linear one.
        if long_edge.is_some() {
            if let Some(hint) = straighten::propose(&rgb, stats_rect) {
                report.add("framing", hint.to_json());
            }
        }
        if let Some(style) = style.filter(|s| s.look.is_some()) {
            rgb = styles::apply(&rgb, style);
            report.add("style", json!({"applied": true, "id": style.id}));
        }
        (rgb, report)
    }

    /// The same frame with every automatic correction switched off.
    pub fn baseline(&mut self, settings: &Settings, long_edge: Option<usize>) -> Image {
        let mut flat = settings.clone();
        flat.enabled.clear();
        for (key, _, _) in TOGGLES.iter() {
            // Hold on to what the user asked for themselves, drop what was
            // decided for them.
            let keep = match *key {
                "white_balance" => settings.wb_rect.is_some(),
                "exposure" => settings.exposure_rect.is_some(),
                _ => false,
            };
            flat.enabled.insert(key.to_string(), if keep { settings.on(key) } else { false });
        }
        let key = format!(
            "{:?}|{}|{:?}|{:?}|{}|{}|{}|{}",
            long_edge,
            settings.framing.key(),
            flat.exposure_rect,
            flat.wb_rect,
            flat.exposure_bias,
            flat.temperature,
            flat.tint,
            flat.preset
        );
        if self.baseline_key == key {
            if let Some(img) = &self.baseline_img {
                return img.clone();
            }
        }
        let (img, _) = self.render(&flat, None, long_edge, true, None);
        self.baseline_key = key;
        self.baseline_img = Some(img.clone());
        img
    }

    fn report_lens(&self, settings: &Settings, measured: &Map<String, Value>, report: &mut Report) {
        let stages = ["vignetting", "distortion", "chromatic aberration"];
        let keys = ["lens_vignetting", "lens_distortion", "lens_tca"];
        let m = match &self.lens_match {
            None => {
                let reason = if self.meta.lens_model.is_empty() {
                    "no_lens_in_exif"
                } else {
                    "lens_not_in_database"
                };
                for stage in stages.iter() {
                    report.add(stage, json!({"applied": false, "reason": reason,
                                             "lens": self.meta.lens_model}));
                }
                return;
            }
            Some(m) => m,
        };
        for (stage, key) in stages.iter().zip(keys.iter()) {
            let mut info = Map::new();
            info.insert("applied".into(), json!(settings.on(key)));
            info.insert("lens".into(), json!(m.lens));
            info.insert("focal".into(), json!(format!("{}mm", trim_num(m.focal))));
            info.insert("aperture".into(), json!(format!("f/{}", trim_num(m.aperture))));
            if let Some(extra) = measured.get(*stage).and_then(|v| v.as_object()) {
                for (k, v) in extra {
                    info.insert(k.clone(), v.clone());
                }
            }
            if !settings.on(key) {
                info.insert("applied".into(), json!(false));
                info.insert("reason".into(), json!("switched_off"));
            }
            report.add(stage, Value::Object(info));
        }
    }

    /// The automatic corrections, with what each one actually decided.
    pub fn toggles(&self, settings: &Settings, report: &Report) -> Value {
        let stage_for = |key: &str| -> &str {
            match key {
                "lens_vignetting" => "vignetting",
                "lens_distortion" => "distortion",
                "lens_tca" => "chromatic aberration",
                "white_balance" => "white balance",
                "exposure" => "exposure",
                "tone_map" => "tone mapping",
                "levels" => "levels",
                "contrast" => "contrast",
                "vibrance" => "vibrance",
                "denoise_chroma" => "colour noise",
                "denoise_luma" => "luminance noise",
                _ => "sharpening",
            }
        };
        let mut out = Vec::new();
        for (key, label, group) in TOGGLES.iter() {
            let info = report.get(stage_for(key));
            let applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(true);
            let reason = info.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            // "Unavailable" means the correction *cannot* run, no lens match,
            // or no data for this correction, as opposed to switched off.
            let available = if key.starts_with("lens_") {
                self.lens_match.is_some() && !(!applied && reason != "switched_off")
            } else {
                true
            };
            out.push(json!({
                "id": key, "label": label, "group": group,
                "enabled": settings.on(key) && available,
                "available": available,
                "detail": detail(key, &info),
            }));
        }
        Value::Array(out)
    }

    pub fn describe(&self) -> Value {
        json!({
            "file": self.filename,
            "width": self.width(),
            "height": self.height(),
            "profile": self.profile.values(),
            "notes": self.profile.notes,
            "lens_calibration": self.lens_match.as_ref().map(|m| json!({
                "lens": m.lens, "focal": round1(m.focal), "aperture": round2(m.aperture)})),
        })
    }
}

fn round1(v: f32) -> f64 {
    ((v as f64) * 10.0).round() / 10.0
}

fn round2(v: f32) -> f64 {
    ((v as f64) * 100.0).round() / 100.0
}

/// "103mm", not "103.0mm", and "10", not "1": only a fractional part may be
/// trimmed.
fn trim_num(v: f32) -> String {
    let s = format!("{}", v);
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// What this correction did, as `{code, params}`.
fn detail(key: &str, info: &Value) -> Value {
    let obj = match info.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => return json!({}),
    };
    if !obj.get("applied").and_then(|v| v.as_bool()).unwrap_or(true) {
        return json!({"code": "reason",
                      "params": {"reason": obj.get("reason").cloned()
                                    .unwrap_or(json!("not_applied"))}});
    }
    let g = |k: &str| obj.get(k).cloned().unwrap_or(Value::Null);
    let lens_bits = json!({"lens": g("lens"), "focal": g("focal"), "aperture": g("aperture")});
    match key {
        "lens_vignetting" => {
            let mut p = lens_bits.as_object().unwrap().clone();
            p.insert("corner_ev".into(), g("corner_gain_ev"));
            json!({"code": "vignetting", "params": p})
        }
        "lens_distortion" | "lens_tca" => json!({"code": "lens", "params": lens_bits}),
        "white_balance" => {
            let gains = g("gains");
            let source = g("source");
            // A neutral reference should need only a modest correction; much
            // more and the patch was not neutral.
            let extreme = match (gains.as_array(), source.as_str()) {
                (Some(a), Some("selection")) if a.len() == 3 => {
                    let v: Vec<f64> = a.iter().map(|x| x.as_f64().unwrap_or(1.0)).collect();
                    let mx = v.iter().cloned().fold(f64::MIN, f64::max);
                    let mn = v.iter().cloned().fold(f64::MAX, f64::min);
                    mx / mn.max(1e-3) > 2.8
                }
                _ => false,
            };
            json!({"code": "white_balance",
                   "params": {"source": source, "gains": gains, "extreme": extreme}})
        }
        "exposure" => json!({"code": "exposure",
                             "params": {"ev": g("gain_ev"), "source": g("source")}}),
        "tone_map" => json!({"code": "tone_map",
                             "params": {"stops": g("scene_stops"),
                                        "compression": g("base_compression")}}),
        "levels" => json!({"code": "levels", "params": {"black": g("black"), "white": g("white")}}),
        "contrast" => json!({"code": "contrast",
                             "params": {"amount": g("s_curve"), "spread": g("spread")}}),
        "vibrance" => json!({"code": "vibrance", "params": {"boost": g("final_boost")}}),
        "denoise_chroma" | "denoise_luma" => json!({"code": "denoise",
                             "params": {"blend": g("blend"), "radius": g("radius")}}),
        "sharpen" => json!({"code": "sharpen",
                            "params": {"amount": g("amount"), "radius": g("radius_px")}}),
        _ => json!({}),
    }
}

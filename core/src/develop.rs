//! One raw file, developed many ways.

use kit::Report;
use crate::decode::{self, Meta};
use crate::geometry::{self, Rect};
use crate::grade::{self, Decisions, ProcessArgs, Settings, Window, TOGGLES};
use crate::lensdb::{self, Database, LensMatch};
use kit::Image;
use crate::profile::{self, CaptureProfile};
use crate::effects;
use crate::straighten;
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
    /// The last "before" rendered, with the settings, size and 3D table it
    /// was rendered from.
    baseline: Option<(Settings, Option<usize>, u64, Image)>,
    /// What an export would decide for these settings, measured for the last
    /// region developed; see `decide_at_full_size`.
    decided_full: Option<(Settings, Decisions)>,
    /// The whole frame at full size, corrected, framed and healed, while
    /// regions of it are being looked at.
    full_frame: Option<(String, Vec<crate::local::Spot>, Image)>,
}

/// A piece of the frame developed at full size; see `render_region`.
pub struct Region {
    pub image: Image,
    /// Where the piece is in the whole framed picture, which is `full_w` by
    /// `full_h` pixels.
    pub x: usize,
    pub y: usize,
    pub full_w: usize,
    pub full_h: usize,
}

/// How far past the edges of a region it is developed, then cut back, for a
/// frame `long` pixels on its long edge.
fn region_margin(long: usize) -> usize {
    (long / 17).max(64)
}

impl Development {
    pub fn open(filename: &str, bytes: &[u8], db: Option<&Database>)
                -> Result<Development, decode::DecodeError> {
        let d = decode::decode(bytes)?;
        Ok(Self::from_frame(filename, d.img, d.meta, decode::collect_exif(bytes), db))
    }

    /// The noise floor of this capture, at native resolution.
    fn native_noise_floor(&mut self) -> f32 {
        if let Some(floor) = self.noise_floor {
            return floor;
        }
        let floor = crate::analyze::centre_noise_floor(&self.linear, 640);
        self.noise_floor = Some(floor);
        floor
    }

    /// How wide this frame's edges are, measured once from the whole picture.
    pub fn focus_width(&mut self) -> Option<f32> {
        if let Some(cached) = self.focus {
            return cached;
        }
        let measured = crate::analyze::focus_width(&self.linear);
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
            baseline: None,
            decided_full: None,
            full_frame: None,
        }
    }

    /// Look the lens up again, in a database that has grown since the file was
    /// opened (the web app loads a mount's lenses once a photograph says which
    /// it needs).
    pub fn use_lenses(&mut self, db: &Database) {
        let m = &self.meta;
        let found = db.find_lens(&m.make, &m.model, &m.lens_model, m.focal_mm, m.aperture, 10.0);
        if found.as_ref().map(|f| &f.entry.model) != self.lens_match.as_ref().map(|f| &f.entry.model) {
            self.lens_match = found;
            self.cache.clear();
            self.baseline = None;
        }
    }

    /// The mount whose lenses `db` would need to find this photograph's lens;
    /// see `Database::missing_mount`.
    pub fn missing_mount(&self, db: &Database) -> Option<String> {
        db.missing_mount(&self.meta.make, &self.meta.model, &self.meta.lens_model)
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
        let key = corrected_key(s, long_edge, crop);
        // The measurements travel with the cached proxy.
        if let Some((_, img, rect, infos)) = self.cache.iter().find(|(k, _, _, _)| *k == key) {
            return (img.clone(), infos.clone(), *rect);
        }
        let mut infos: Map<String, Value> = Map::new();

        let mut src = match long_edge {
            None => self.linear.clone(),
            Some(e) => kit::thumbnail(&self.linear, e),
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

    /// Develop the picture.
    pub fn render(&mut self, settings: &Settings,
                  long_edge: Option<usize>, crop: bool,
                  mut progress: Option<&mut dyn FnMut(f32, &str)>)
                  -> (Image, Report) {
        // A full-size render has no room for a second full-size frame kept
        // for looking around at 100 %.
        if long_edge.is_none() {
            self.release_region();
        }
        let mut report = Report::new();
        let (src, lens_info, eff_crop) = self.corrected(settings, long_edge, crop, &mut progress);
        self.report_lens(settings, &lens_info, &mut report);

        let scale = match long_edge {
            None => 1.0,
            Some(_) => src.w.max(src.h) as f32 / self.linear.w.max(self.linear.h) as f32,
        };

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

        let native_noise_floor = self.native_noise_floor();
        // The width was measured at a fixed working size, so the blur it
        // describes is that many pixels *there*.
        let measured_width = self.focus_width();
        let focus_sigma = self.focus_sigma_at(src.w.max(src.h));
        let args = ProcessArgs {
            iso: self.profile.iso,
            sharpen_sigma: self.profile.sharpen_sigma,
            noise_prior: self.profile.noise_prior,
            stats_rect,
            render_scale: scale,
            preset: tuned,
            native_noise_floor,
            focus_sigma,
            window: Window::whole(&src),
            decided: None,
            progress: graded_progress.as_mut().map(|f| &mut **f as &mut dyn FnMut(f32, &str)),
        };
        let (rgb, _) = grade::process(&src, settings, args, &mut report);

        // What the frame measured, said out loud whichever way it came out.
        report.add("focus", match measured_width {
            None => json!({"measured": false, "verdict": "unknown"}),
            Some(w) => json!({"measured": true,
                "verdict": if w >= crate::analyze::FOCUS_SOFT { "soft" }
                           else if w <= crate::analyze::FOCUS_SHARP { "sharp" }
                           else { "slightly_soft" },
                "edge_px": kit::round_to(w, 2),
                "blur_px": kit::round_to(crate::analyze::focus_sigma(w), 2)}),
        });

        // Framing is the one step that had nothing of its own to propose, so
        // the measurement rides along with the preview it was taken from,
        // against the graded frame rather than the linear one.
        if long_edge.is_some() {
            if let Some(hint) = straighten::propose(&rgb, stats_rect) {
                report.add("framing", hint.to_json());
            }
        }
        let window = Window::whole(&rgb);
        (finish(rgb, settings, window, &mut report), report)
    }

    /// The blur focus recovery aims at, in pixels of a render `long` pixels on
    /// its long edge.
    pub fn release_region(&mut self) {
        self.full_frame = None;
        self.decided_full = None;
    }

    fn decide_at_full_size(&mut self, full: &Image, settings: &Settings) -> Decisions {
        if let Some((s, d)) = &self.decided_full {
            if s == settings {
                return d.clone();
            }
        }
        let proxy = kit::thumbnail(full, 1024);
        let args = ProcessArgs {
            iso: self.profile.iso,
            sharpen_sigma: self.profile.sharpen_sigma,
            noise_prior: self.profile.noise_prior,
            stats_rect: None,
            render_scale: 1.0,
            preset: profile::tune(&settings.preset_obj(), &self.profile),
            native_noise_floor: self.native_noise_floor(),
            focus_sigma: 0.0,
            window: Window::whole(&proxy),
            decided: None,
            progress: None,
        };
        let (_, decided) = grade::process(&proxy, settings, args, &mut Report::new());
        self.decided_full = Some((settings.clone(), decided.clone()));
        decided
    }

    fn focus_sigma_at(&mut self, long: usize) -> f32 {
        match self.focus_width() {
            Some(w) if w >= crate::analyze::FOCUS_SOFT => {
                let at_work = crate::analyze::focus_sigma(w);
                at_work * (long as f32 / crate::analyze::FOCUS_LONG_EDGE as f32).min(4.0)
            }
            _ => 0.0,
        }
    }

    /// A piece of the framed picture at full size, as it would be in an export.
    pub fn render_region(&mut self, settings: &Settings, centre: (f32, f32), want: (usize, usize),
                         mut progress: Option<&mut dyn FnMut(f32, &str)>) -> Region {
        let key = corrected_key(settings, None, true);
        let fresh = !matches!(&self.full_frame, Some((k, spots, _)) if *k == key && *spots == settings.spots);
        if fresh {
            self.full_frame = None;
            let (mut full, _, _) = self.corrected(settings, None, true, &mut progress);
            crate::local::heal(&mut full, &settings.spots, None);
            self.full_frame = Some((key, settings.spots.clone(), full));
        }
        let (key, spots, full) = self.full_frame.take().expect("made above");
        // Healed above, on the whole frame.
        let healed = Settings { spots: Vec::new(), ..settings.clone() };
        let decided = self.decide_at_full_size(&full, &healed);
        let (fw, fh) = (full.w, full.h);
        let (w, h) = (want.0.clamp(1, fw), want.1.clamp(1, fh));
        let place = |c: f32, size: usize, whole: usize| {
            ((c.clamp(0.0, 1.0) * whole as f32) as usize).saturating_sub(size / 2).min(whole - size)
        };
        let (x, y) = (place(centre.0, w, fw), place(centre.1, h, fh));
        let margin = region_margin(fw.max(fh));
        let (x0, y0) = (x.saturating_sub(margin), y.saturating_sub(margin));
        let (x1, y1) = ((x + w + margin).min(fw), (y + h + margin).min(fh));
        let piece = full.crop(x0, y0, x1 - x0, y1 - y0);
        self.full_frame = Some((key, spots, full));

        let window = Window { x: x0, y: y0, w: fw, h: fh };
        let mut report = Report::new();
        let mut graded_progress = progress.map(|cb| {
            let f: Box<dyn FnMut(f32, &str) + '_> =
                Box::new(move |f: f32, code: &str| cb(0.25 + 0.75 * f, code));
            f
        });
        let args = ProcessArgs {
            iso: self.profile.iso,
            sharpen_sigma: self.profile.sharpen_sigma,
            noise_prior: self.profile.noise_prior,
            stats_rect: None,
            render_scale: 1.0,
            preset: profile::tune(&settings.preset_obj(), &self.profile),
            native_noise_floor: self.native_noise_floor(),
            focus_sigma: self.focus_sigma_at(fw.max(fh)),
            window,
            decided: Some(decided),
            progress: graded_progress.as_mut().map(|f| &mut **f as &mut dyn FnMut(f32, &str)),
        };
        let (rgb, _) = grade::process(&piece, &healed, args, &mut report);
        let rgb = finish(rgb, settings, window, &mut report);
        Region { image: rgb.crop(x - x0, y - y0, w, h), x, y, full_w: fw, full_h: fh }
    }
}

/// What a lens-corrected, framed frame depends on, and nothing else.
fn corrected_key(s: &Settings, long_edge: Option<usize>, crop: bool) -> String {
    format!(
        "{:?}|{}|{}|{}|{}|{}",
        long_edge,
        s.on("lens_vignetting"),
        s.on("lens_distortion"),
        s.on("lens_tca"),
        s.framing.key(),
        crop
    )
}

/// Taste, after the pipeline's own work.
fn finish(mut rgb: Image, settings: &Settings, window: Window, report: &mut Report) -> Image {
    {
        // Taste, in the order the darkroom had it.
        if !settings.mixer.is_identity() {
            rgb = crate::mixer::apply(&rgb, &settings.mixer);
        }
        report.add("mixer", json!({"applied": !settings.mixer.is_identity()}));

        effects::monochrome(&mut rgb, settings.monochrome);

        let counts = settings.curves.counts();
        report.add("curves", if settings.curves.is_identity() {
            json!({"applied": false})
        } else {
            rgb = settings.curves.apply(&rgb);
            json!({"applied": true, "points": {"rgb": counts[0], "r": counts[1],
                                               "g": counts[2], "b": counts[3]}})
        });

        // The 3D table is the end of the colour stack and runs after the
        // curves, which is both where the format expects to sit and the only
        // place it can sit.
        let lut_applied = crate::lut::apply_current(&mut rgb, settings.lut);
        report.add("lut", json!({"applied": lut_applied,
                                 "strength": kit::round_to(settings.lut, 3)}));

        effects::vignette_in(&mut rgb, settings.vignette, window);
        effects::grain_in(&mut rgb, settings.grain, window);
        report.add("effects", json!({
            "monochrome": kit::round_to(settings.monochrome, 3),
            "vignette": kit::round_to(settings.vignette, 3),
            "grain": kit::round_to(settings.grain, 3)}));
    }
    rgb
}

impl Development {

    /// The same frame with every automatic correction switched off.
    pub fn baseline(&mut self, settings: &Settings, long_edge: Option<usize>) -> Image {
        let mut flat = settings.clone();
        flat.enabled.clear();
        // The grade comes off the "before" for the same reason the look does.
        flat.curves = crate::curve::Stack::identity();
        for (key, ..) in TOGGLES.iter() {
            // Hold on to what the user asked for themselves, drop what was
            // decided for them.
            let keep = match *key {
                "white_balance" => settings.wb_rect.is_some(),
                "exposure" => settings.exposure_rect.is_some(),
                _ => false,
            };
            flat.enabled.insert(key.to_string(), if keep { settings.on(key) } else { false });
        }
        // Cached on everything the render reads, compared whole rather than
        // listed field by field.
        let lut = crate::lut::generation();
        if let Some((s, edge, g, img)) = &self.baseline {
            if *s == flat && *edge == long_edge && *g == lut {
                return img.clone();
            }
        }
        let (img, _) = self.render(&flat, long_edge, true, None);
        self.baseline = Some((flat, long_edge, lut, img.clone()));
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
                    report.skipped(stage, reason, json!({"lens": self.meta.lens_model}));
                }
                return;
            }
            Some(m) => m,
        };
        for (stage, key) in stages.iter().zip(keys.iter()) {
            let info = kit::with(
                json!({"applied": settings.on(key), "lens": m.lens,
                       "focal": format!("{}mm", trim_num(m.focal)),
                       "aperture": format!("f/{}", trim_num(m.aperture))}),
                measured.get(*stage).cloned().unwrap_or(Value::Null));
            if settings.on(key) {
                report.add(stage, info);
            } else {
                report.skipped(stage, "switched_off", info);
            }
        }
    }

    /// The automatic corrections, with what each one actually decided.
    pub fn toggles(&self, settings: &Settings, report: &Report) -> Value {
        let mut out = Vec::new();
        for (key, label, group, stage) in TOGGLES.iter() {
            let info = report.get(stage);
            let applied = info.get("applied").and_then(|v| v.as_bool()).unwrap_or(true);
            let reason = info.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            // "Unavailable" means the correction *cannot* run, no lens match,
            // or no data for this correction, as opposed to switched off.
            let available = if *group == "Lens" {
                self.lens_match.is_some() && (applied || reason == "switched_off")
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
                "lens": m.lens, "focal": kit::round_to(m.focal, 1), "aperture": kit::round_to(m.aperture, 2)})),
        })
    }
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
        "refocus" => json!({"code": "refocus",
                            "params": {"amount": g("amount"), "blur": g("sigma_px")}}),
        "sharpen" => json!({"code": "sharpen",
                            "params": {"amount": g("amount"), "radius": g("radius_px")}}),
        _ => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn development() -> Development {
        let mut img = Image::new(96, 64);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 96) as f32 / 96.0, (i / 96) as f32 / 64.0);
            *px = [0.05 + 0.3 * x, 0.08 + 0.2 * y, 0.04 + 0.1 * x * y];
        }
        Development::from_frame("test", img, Meta::default(), Vec::new(), None)
    }

    /// The "before" is cached, and the cache has to notice every setting the
    /// render reads.
    #[test]
    fn the_before_follows_every_setting_it_renders() {
        let mut dev = development();
        let plain = Settings::default();
        let a = dev.baseline(&plain, Some(64));
        assert_eq!(dev.baseline(&plain, Some(64)).d, a.d, "the same settings rendered twice");
        let mut vignetted = plain.clone();
        vignetted.vignette = 0.8;
        assert_ne!(dev.baseline(&vignetted, Some(64)).d, a.d, "a stale before for the vignette");
        let mut toned = plain.clone();
        toned.highlights = -0.8;
        assert_ne!(dev.baseline(&toned, Some(64)).d, a.d, "a stale before for the tone zones");
    }

    /// A piece developed on its own has to be the same pixels the whole frame
    /// has there.
    #[test]
    fn a_region_is_the_same_pixels_as_the_whole() {
        let mut img = Image::new(640, 420);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 640) as f32, (i / 640) as f32);
            let texture = 0.04 * ((x * 0.37).sin() * (y * 0.23).cos());
            *px = [0.05 + 0.3 * x / 640.0 + texture, 0.08 + 0.2 * y / 420.0 + texture,
                   0.06 + 0.1 * (x + y) / 1060.0];
        }
        let mut dev = Development::from_frame("test", img, Meta::default(), Vec::new(), None);
        let s = Settings::from_json(&json!({"clarity": 0.6, "denoise": 0.5, "vignette": 0.7,
            "grain": 0.5, "filters": [{"kind": "linear", "a": [0.5, 0.0], "b": [0.5, 0.7],
            "exposure": -0.8}]}));
        let whole = dev.render_region(&s, (0.5, 0.5), (10_000, 10_000), None);
        assert_eq!((whole.image.w, whole.image.h, whole.x, whole.y), (640, 420, 0, 0));
        let piece = dev.render_region(&s, (0.7, 0.3), (160, 120), None);
        assert_eq!((piece.full_w, piece.full_h), (640, 420));
        let mut worst = 0f32;
        for y in 0..piece.image.h {
            for x in 0..piece.image.w {
                for c in 0..3 {
                    let a = piece.image.d[(y * piece.image.w + x) * 3 + c];
                    let b = whole.image.d[((y + piece.y) * 640 + x + piece.x) * 3 + c];
                    worst = worst.max((a - b).abs());
                }
            }
        }
        assert!(worst < 1.0 / 1024.0, "the piece is {worst} off the whole frame");
    }

    /// Each toggle reports what its own stage did, not a neighbour's.
    #[test]
    fn every_toggle_reads_its_own_stage() {
        let mut dev = development();
        let settings = Settings::default();
        let (_, report) = dev.render(&settings, Some(64), true, None);
        let toggles = dev.toggles(&settings, &report);
        let refocus = toggles.as_array().unwrap().iter().find(|t| t["id"] == "refocus").unwrap();
        assert_eq!(refocus["detail"]["code"], "reason");
        let reason = refocus["detail"]["params"]["reason"].as_str().unwrap();
        assert!(reason == "sharp_enough" || reason == "not_asked", "refocus said {reason}");
        for (_, _, _, stage) in TOGGLES.iter() {
            assert!(!report.get(stage).as_object().unwrap().is_empty(), "no report stage called {stage}");
        }
    }
}

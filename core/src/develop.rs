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
use std::borrow::Cow;

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
    /// The lens as found from the file, when nobody has chosen another.
    pub lens_match: Option<LensMatch>,
    /// The camera and its mount's calibrated lenses, to choose a lens from.
    lenses: Option<Database>,
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
    /// The last preview as it stood before `finish`, with what it was
    /// developed from, so that moving a wheel, a curve or the mixer develops only
    /// the end of the pipeline again; see `Graded`.
    graded: Option<Graded>,
    /// The sky and subject masks found for a framing, keyed on it.
    masks: Option<(String, crate::masks::Masks)>,
    /// Whether deep noise reduction was asked for when the file was opened,
    /// and what it measured if it could run.
    deep_denoise: Option<Option<crate::denoise::Noise>>,
}

/// A preview developed up to `finish`, and everything that went into it.
struct Graded {
    /// The settings with everything `finish` reads put back to its default.
    settings: Settings,
    long_edge: Option<usize>,
    crop: bool,
    /// Which of the sky and subject masks there were: the subject arrives
    /// once its model is loaded, with no setting changing.
    masks: (bool, bool),
    image: Image,
    report: Report,
}

/// The settings with what only `finish` reads put back to its defaults: two
/// renders that agree on these agree on everything before `finish`.
fn before_finish(s: &Settings) -> Settings {
    let d = Settings::default();
    Settings {
        mixer: d.mixer,
        monochrome: d.monochrome,
        wheels: d.wheels,
        curves: d.curves,
        lut: d.lut,
        vignette: d.vignette,
        grain: d.grain,
        ..s.clone()
    }
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
        Self::open_with(filename, bytes, db, None)
    }

    /// `open`, with deep noise reduction on the raw mosaic when `denoiser` is
    /// given; `describe` says whether it could run.
    pub fn open_with(filename: &str, bytes: &[u8], db: Option<&Database>,
                     denoiser: Option<decode::Denoiser>) -> Result<Development, decode::DecodeError> {
        let asked = denoiser.is_some();
        let d = decode::decode_with(bytes, denoiser)?;
        let mut dev = Self::from_frame(filename, d.img, d.meta, decode::collect_exif(bytes), db);
        dev.deep_denoise = asked.then_some(d.denoised);
        // The ISO no longer says how noisy the frame is. Only the noise
        // measured on the cleaned frame decides from here, or the usual noise
        // reduction would smear what the network already cleaned.
        if d.denoised.is_some() {
            dev.profile.noise_prior = 0.0;
        }
        Ok(dev)
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
        let crop_factor = db.and_then(|db| db.find_camera(&meta.make, &meta.model)).map(|c| c.crop);
        let lens_match = find_lens(db, &meta);
        let lenses = db.and_then(|db| mount_lenses(db, &meta));
        let prof = profile::derive(&meta, w, h, crop_factor);
        Development {
            filename: filename.to_string(),
            linear,
            meta,
            exif,
            noise_floor: None,
            profile: prof,
            lens_match,
            lenses,
            cache: Vec::new(),
            focus: None,
            baseline: None,
            decided_full: None,
            full_frame: None,
            graded: None,
            masks: None,
            deep_denoise: None,
        }
    }

    /// Look the lens up again, in a database that has grown since the file was
    /// opened (the web app loads a mount's lenses once a photograph says which
    /// it needs).
    pub fn use_lenses(&mut self, db: &Database) {
        let found = find_lens(Some(db), &self.meta);
        let lenses = mount_lenses(db, &self.meta);
        let which = |m: &Option<LensMatch>| m.as_ref().map(|m| (m.source(), m.entry.model.clone()));
        let count = |l: &Option<Database>| l.as_ref().map_or(0, |l| l.lenses.len());
        if count(&lenses) != count(&self.lenses) {
            self.lenses = lenses;
        }
        if which(&found) != which(&self.lens_match) {
            self.lens_match = found;
            self.cache.clear();
            self.baseline = None;
            self.graded = None;
            self.full_frame = None;
            self.decided_full = None;
            self.masks = None;
        }
    }

    /// The mount whose lenses `db` would need to find this photograph's lens;
    /// see `Database::missing_mount`.
    pub fn missing_mount(&self, db: &Database) -> Option<String> {
        db.missing_mount(&self.meta.make, &self.meta.model)
    }

    /// The lens these settings correct for: the one chosen, or else the one
    /// found from the file.
    fn lens_for(&self, s: &Settings) -> Option<LensMatch> {
        let m = &self.meta;
        let chosen = match s.lens.as_str() {
            "" => None,
            "camera" => camera_lens(self.lenses.as_ref(), m),
            model => self.lenses.as_ref().and_then(|db| {
                db.find_chosen(&m.make, &m.model, model, m.focal_mm, m.aperture, focus_distance(m))
            }),
        };
        chosen.or_else(|| self.lens_match.clone())
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

        // Borrowed until a stage has to write to it, so a full-size frame
        // is only copied when nothing else makes a new one.
        let mut src = match self.working_edge(s, long_edge, crop) {
            None => Cow::Borrowed(&self.linear),
            Some(e) => Cow::Owned(kit::thumbnail(&self.linear, e)),
        };
        if let Some(m) = &self.lens_for(s) {
            if s.on("lens_vignetting") {
                if let Some(cb) = progress.as_mut() {
                    cb(0.02, "vignetting");
                }
                let info = lensdb::apply_vignetting(src.to_mut(), m);
                infos.insert("vignetting".into(), info);
            }
            if s.on("lens_distortion") || s.on("lens_tca") {
                if let Some(cb) = progress.as_mut() {
                    cb(0.05, "geometry");
                }
                let (out, geo) = lensdb::apply_geometry(&src, m, s.on("lens_distortion"),
                                                        s.on("lens_tca"));
                src = Cow::Owned(out);
                infos.insert("distortion".into(), geo.clone());
                infos.insert("chromatic aberration".into(), geo);
            }
        }
        // Framing comes after lens correction (distortion must be undone in the
        // original frame) and before everything tonal, so metering and the auto
        // levels only ever see the picture that will actually be kept.
        let eff = geometry::effective_crop(src.w, src.h, &s.framing);
        if !s.framing.is_identity() {
            src = Cow::Owned(geometry::apply(&src, &s.framing, crop));
        }
        let src = src.into_owned();
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
        let masks = self.masks_for(settings);
        let mask_kinds = (masks.get(crate::masks::Kind::Sky).is_some(),
                          masks.get(crate::masks::Kind::Subject).is_some());
        let upstream = before_finish(settings);
        if let Some(g) = &self.graded {
            if long_edge.is_some() && g.long_edge == long_edge && g.crop == crop
                && g.masks == mask_kinds && g.settings == upstream {
                let mut report = g.report.clone();
                let window = Window::whole(&g.image);
                return (finish(g.image.clone(), settings, window, &mut report), report);
            }
        }
        let mut report = Report::new();
        let (src, lens_info, eff_crop) = self.corrected(settings, long_edge, crop, &mut progress);
        self.report_lens(settings, &lens_info, &mut report);

        // Of the whole frame, not of what the framing kept of it.
        let native = self.linear.w.max(self.linear.h);
        let scale = self.working_edge(settings, long_edge, crop)
            .map_or(1.0, |e| (e as f32 / native as f32).min(1.0));

        // When the canvas is shown uncropped, the estimators must still measure
        // only the rectangle being kept, see geometry::stats_view.
        let stats_rect = if crop { None } else { eff_crop };

        let mut graded_progress = after_lens(progress);
        let measured_width = self.focus_width();
        let args = ProcessArgs {
            stats_rect,
            render_scale: scale,
            focus_sigma: self.focus_sigma_at((native as f32 * scale).round() as usize),
            progress: graded_progress.as_mut().map(|f| &mut **f as &mut dyn FnMut(f32, &str)),
            ..self.process_args(settings, &masks, Window::whole(&src))
        };
        let (rgb, _) = grade::process(src, settings, args, &mut report);

        report.add("focus", match measured_width {
            None => json!({"measured": false, "verdict": "unknown"}),
            Some(w) => json!({"measured": true,
                "verdict": if w >= crate::analyze::FOCUS_SOFT { "soft" }
                           else if w <= crate::analyze::FOCUS_SHARP { "sharp" }
                           else { "slightly_soft" },
                "edge_px": kit::round_to(w, 2),
                "blur_px": kit::round_to(crate::analyze::focus_sigma(w), 2)}),
        });

        // The straightening proposal is measured on the graded preview.
        if long_edge.is_some() {
            if let Some(hint) = straighten::propose(&rgb, stats_rect) {
                report.add("framing", hint.to_json());
            }
        }
        // Only previews are kept: a full-size frame is too big to hold twice.
        self.graded = long_edge.map(|_| Graded {
            settings: upstream, long_edge, crop, masks: mask_kinds,
            image: rgb.clone(), report: report.clone(),
        });
        let window = Window::whole(&rgb);
        (finish(rgb, settings, window, &mut report), report)
    }

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
        let masks = self.masks_for(settings);
        let args = self.process_args(settings, &masks, Window::whole(&proxy));
        let (_, decided) = grade::process(proxy, settings, args, &mut Report::new());
        self.decided_full = Some((settings.clone(), decided.clone()));
        decided
    }

    /// What every development hands `grade::process`, for a full-size render
    /// with nothing decided yet.
    fn process_args<'a>(&mut self, settings: &Settings, masks: &'a crate::masks::Masks,
                        window: Window) -> ProcessArgs<'a> {
        ProcessArgs {
            iso: self.profile.iso,
            sharpen_sigma: self.profile.sharpen_sigma,
            noise_prior: self.profile.noise_prior,
            stats_rect: None,
            render_scale: 1.0,
            preset: profile::tune(&settings.preset_obj(), &self.profile),
            native_noise_floor: self.native_noise_floor(),
            focus_sigma: 0.0,
            window,
            decided: None,
            masks,
            progress: None,
        }
    }

    /// How large to make the frame before framing it, for a picture `long_edge`
    /// on its long edge once framed. A crop is cut from a frame up to three
    /// times larger, or a tight one would come out a fraction of the size.
    fn working_edge(&self, s: &Settings, long_edge: Option<usize>, crop: bool) -> Option<usize> {
        let e = long_edge?;
        let (w, h) = (self.linear.w, self.linear.h);
        let native = w.max(h);
        let Some((_, _, cw, ch)) = geometry::effective_crop(w, h, &s.framing).filter(|_| crop) else {
            return Some(e);
        };
        let (canvas_h, canvas_w) = geometry::canvas_size(w, h, &s.framing);
        let kept = (cw * canvas_w as f32).max(ch * canvas_h as f32).max(1.0);
        let wanted = (e as f32 * native as f32 / kept).round() as usize;
        Some(wanted.clamp(e, 3 * e))
    }

    /// The blur focus recovery aims at, in pixels of a render whose whole
    /// frame is `long` pixels on its long edge, cropped or not. The width was
    /// measured at a fixed working size, so it is
    /// scaled from there.
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
        let masks = self.masks_for(settings);
        let mut report = Report::new();
        let mut graded_progress = after_lens(progress);
        let args = ProcessArgs {
            focus_sigma: self.focus_sigma_at(self.linear.w.max(self.linear.h)),
            decided: Some(decided),
            progress: graded_progress.as_mut().map(|f| &mut **f as &mut dyn FnMut(f32, &str)),
            ..self.process_args(settings, &masks, window)
        };
        let (rgb, _) = grade::process(piece, &healed, args, &mut report);
        let rgb = finish(rgb, settings, window, &mut report);
        Region { image: rgb.crop(x - x0, y - y0, w, h), x, y, full_w: fw, full_h: fh }
    }
}

/// The lens in the database, by the maker's lens number first and its name
/// second, or failing both the corrections the camera wrote into the file.
fn find_lens(db: Option<&Database>, meta: &Meta) -> Option<LensMatch> {
    let (f, a, d) = (meta.focal_mm, meta.aperture, focus_distance(meta));
    let found = db.and_then(|db| {
        meta.lens_identified.as_ref()
            .and_then(|(maker, lens)| db.find_identified(&meta.make, &meta.model, maker, lens, f, a, d))
            .or_else(|| db.find_lens(&meta.make, &meta.model, &meta.lens_model, f, a, d))
    });
    found.or_else(|| camera_lens(db, meta))
}

/// The corrections the camera wrote into the file, as a lens.
fn camera_lens(db: Option<&Database>, meta: &Meta) -> Option<LensMatch> {
    let crop = db.and_then(|db| db.find_camera(&meta.make, &meta.model)).map_or(1.0, |c| c.crop);
    let name = meta.lens_identified.as_ref()
        .map_or(meta.lens_model.clone(), |(maker, lens)| format!("{maker} {lens}"));
    meta.embedded_lens.clone().map(|e| LensMatch::from_embedded(
        e, &meta.model, &name, crop, meta.focal_mm, meta.aperture))
}

/// The camera and the calibrated lenses of its mount, on their own.
fn mount_lenses(db: &Database, meta: &Meta) -> Option<Database> {
    let camera = db.find_camera(&meta.make, &meta.model)?.clone();
    let lenses = db.lenses_for(&meta.make, &meta.model);
    (!lenses.is_empty()).then(|| Database { cameras: vec![camera], lenses, mounts: None })
}

/// Where the lens was focused, in metres, for picking a vignetting
/// calibration: 10 when the file does not say, 1000 for infinity.
fn focus_distance(meta: &Meta) -> f32 {
    match meta.focus_m {
        d if d.is_infinite() => 1000.0,
        d if d > 0.0 => d,
        _ => 10.0,
    }
}

/// Lens work is the first quarter of a render; grading is the rest.
fn after_lens<'a>(progress: Option<&'a mut dyn FnMut(f32, &str)>)
                  -> Option<Box<dyn FnMut(f32, &str) + 'a>> {
    progress.map(|cb| Box::new(move |f: f32, code: &str| cb(0.25 + 0.75 * f, code)) as Box<_>)
}

/// What a lens-corrected, framed frame depends on, and nothing else.
fn corrected_key(s: &Settings, long_edge: Option<usize>, crop: bool) -> String {
    format!(
        "{:?}|{}|{}|{}|{}|{}|{}",
        long_edge,
        s.lens,
        s.on("lens_vignetting"),
        s.on("lens_distortion"),
        s.on("lens_tca"),
        s.framing.key(),
        crop
    )
}

/// Taste, after the pipeline's own work.
fn finish(mut rgb: Image, settings: &Settings, window: Window, report: &mut Report) -> Image {
    if !settings.mixer.is_identity() {
        rgb = crate::mixer::apply(&rgb, &settings.mixer);
    }
    report.add("mixer", json!({"applied": !settings.mixer.is_identity()}));

    effects::monochrome(&mut rgb, settings.monochrome);

    // After monochrome, so the wheels can tone a black-and-white frame.
    let wheels = &settings.wheels;
    if !wheels.is_identity() {
        let (skin, protect) = (settings.skin, settings.skin_protect);
        rgb = crate::wheels::apply(&rgb, wheels, |hue, chroma| protect * skin.likeness(hue, chroma));
    }
    report.add("wheels", json!({"applied": !wheels.is_identity()}));

    let counts = settings.curves.counts();
    report.add("curves", if settings.curves.is_identity() {
        json!({"applied": false})
    } else {
        settings.curves.apply_to(&mut rgb);
        json!({"applied": true, "points": {"rgb": counts[0], "r": counts[1],
                                           "g": counts[2], "b": counts[3]}})
    });

    // The 3D table ends the colour stack, after the curves, where the format
    // expects to sit.
    let lut_applied = crate::lut::apply_current(&mut rgb, settings.lut);
    report.add("lut", json!({"applied": lut_applied,
                             "strength": kit::round_to(settings.lut, 3)}));

    effects::vignette_in(&mut rgb, settings.vignette, window);
    effects::grain_in(&mut rgb, settings.grain, window);
    report.add("effects", json!({
        "monochrome": kit::round_to(settings.monochrome, 3),
        "vignette": kit::round_to(settings.vignette, 3),
        "grain": kit::round_to(settings.grain, 3)}));
    rgb
}

impl Development {
    /// The same frame with every automatic correction switched off.
    pub fn baseline(&mut self, settings: &Settings, long_edge: Option<usize>) -> Image {
        let mut flat = settings.clone();
        flat.enabled.clear();
        // The grade comes off the "before": it is taste, not a correction.
        flat.curves = crate::curve::Stack::identity();
        flat.wheels = crate::wheels::Wheels::default();
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

    /// The masks `settings`' filters follow, found for its framing the first
    /// time one is asked for and kept while the framing holds. The subject
    /// needs the segmenter (`models`); without it that mask is left out.
    fn masks_for(&mut self, settings: &Settings) -> crate::masks::Masks {
        use crate::masks::Kind;
        let wanted: Vec<Kind> = settings.filters.iter().filter_map(|f| f.mask()).collect();
        if wanted.is_empty() {
            return crate::masks::Masks::default();
        }
        let key = corrected_key(settings, None, true);
        if self.masks.as_ref().is_some_and(|(k, _)| *k != key) {
            self.masks = None;
        }
        let mut have = self.masks.take().map(|(_, m)| m).unwrap_or_default();
        let missing = |have: &crate::masks::Masks, k: Kind| {
            wanted.contains(&k) && have.get(k).is_none()
                && (k != Kind::Subject || crate::models::loaded("segmenter"))
        };
        if missing(&have, Kind::Sky) || missing(&have, Kind::Subject) {
            // The framed picture as developed, with no filters on it: what the
            // masks are of.
            let plain = Settings { filters: Vec::new(), ..settings.clone() };
            let (picture, _) = self.render(&plain, Some(crate::masks::SKY_EDGE.max(512)), true, None);
            if missing(&have, Kind::Sky) {
                let mut sky = crate::masks::sky(&picture);
                // Whatever the segmenter calls a subject is not sky, however
                // smooth and light it is; it is asked only when already loaded.
                if let Ok(subject) = crate::models::with("segmenter",
                                                         |g| crate::masks::subject(&picture, g)) {
                    let subject = kit::resize_plane(&subject, sky.w, sky.h);
                    sky.d.iter_mut().zip(&subject.d).for_each(|(s, p)| *s *= 1.0 - p);
                }
                have.sky = Some(sky);
            }
            if missing(&have, Kind::Subject) {
                have.subject = crate::models::with("segmenter", |g| crate::masks::subject(&picture, g)).ok();
            }
        }
        self.masks = Some((key, have.clone()));
        have
    }

    /// One mask as found for these settings' framing, for the interface to
    /// show over the picture; `None` when it cannot be found.
    pub fn mask(&mut self, settings: &Settings, kind: crate::masks::Kind) -> Option<kit::Plane> {
        let probe = crate::local::Filter {
            shape: crate::local::Shape::Mask { kind, invert: false },
            exposure: 0.0, temperature: 0.0, saturation: 0.0,
        };
        let s = Settings { filters: vec![probe], ..settings.clone() };
        self.masks_for(&s).get(kind).cloned()
    }

    /// Wheels that bring this photograph's colour toward a reference picture's
    /// (display-linear), with everything else in `settings` as it is. Fitted
    /// on the finished picture, so the look and curves after the wheels are
    /// accounted for, and fitted twice, the second time on what the first
    /// left over, since those stages bend what goes through them.
    pub fn match_colour(&mut self, settings: &Settings, reference: &Image) -> crate::wheels::Wheels {
        const EDGE: usize = 384;
        let reference = kit::thumbnail(reference, EDGE);
        let mut s = settings.clone();
        s.wheels = crate::wheels::Wheels::default();
        let (plain, _) = self.render(&s, Some(EDGE), true, None);
        let first = crate::wheels::fit(&plain, &reference);
        s.wheels = first.clone();
        let (once, _) = self.render(&s, Some(EDGE), true, None);
        let rest = crate::wheels::fit(&once, &reference);
        crate::wheels::add(&first, &rest)
    }

    /// The skin of this photograph's faces, as developed with `settings`, and
    /// how many faces it was read from; `None` when there are no faces.
    pub fn measure_skin(&mut self, settings: &Settings, detector: &critic::graph::Graph)
                        -> Option<(grade::Skin, usize)> {
        // The colour skin has where vibrance and the wheels see it: before the
        // grade does anything to it.
        let mut s = settings.clone();
        s.wheels = crate::wheels::Wheels::default();
        s.mixer = crate::mixer::Mixer::default();
        s.curves = crate::curve::Stack::identity();
        s.lut = 0.0;
        s.monochrome = 0.0;
        let (img, _) = self.render(&s, Some(crate::skin::SEARCH_EDGE), true, None);
        crate::skin::measure(&img, detector)
    }

    fn report_lens(&self, settings: &Settings, measured: &Map<String, Value>, report: &mut Report) {
        let stages = ["vignetting", "distortion", "chromatic aberration"];
        let keys = ["lens_vignetting", "lens_distortion", "lens_tca"];
        let lens = self.lens_for(settings);
        let m = match &lens {
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
                json!({"applied": settings.on(key), "lens": m.lens, "source": m.source(),
                       "guessed": m.guessed,
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
                self.lens_for(settings).is_some() && (applied || reason == "switched_off")
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
                "lens": m.lens, "focal": kit::round_to(m.focal, 1), "aperture": kit::round_to(m.aperture, 2),
                "source": m.source(), "guessed": m.guessed})),
            // What a lens can be chosen from: the mount's calibrated lenses,
            // and whether the camera wrote corrections of its own.
            "lens_choices": self.lenses.as_ref().map_or(Vec::new(), |db| db.lenses.iter()
                .map(|l| json!({"model": l.model, "name": lensdb::full_name(l)})).collect()),
            "lens_camera": self.meta.embedded_lens.is_some(),
            "lens_identified": self.meta.lens_identified.as_ref().map(|(maker, lens)| format!("{maker} {lens}")),
            "focus_m": match self.meta.focus_m {
                d if d.is_infinite() => json!("inf"),
                d if d > 0.0 => json!(kit::round_to(d, 2)),
                _ => Value::Null,
            },
            // Null when not asked for; false when the sensor is not one it
            // knows (not a 2 x 2 Bayer mosaic).
            "deep_denoise": self.deep_denoise.map(|n| n.is_some()),
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
    let lens_bits = json!({"lens": g("lens"), "focal": g("focal"), "aperture": g("aperture"),
                           "source": g("source"), "guessed": g("guessed")});
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
        // The wheels are the grade, and the "before" is without it.
        let mut graded = plain.clone();
        graded.wheels.global = crate::wheels::Wheel { hue: 40.0, amount: 1.0, lum: 0.0 };
        assert_eq!(dev.baseline(&graded, Some(64)).d, a.d, "the wheels reached the before");
    }

    /// A preview developed again with only the grade changed comes out the
    /// same as one developed from nothing, and a change before the grade is
    /// not missed.
    #[test]
    fn a_kept_preview_gives_the_same_picture() {
        let mut graded = Settings::default();
        graded.wheels.shadows = crate::wheels::Wheel { hue: 200.0, amount: 0.6, lum: 0.4 };
        graded.vignette = 0.5;
        let fresh = development().render(&graded, Some(64), true, None).0;
        let mut dev = development();
        dev.render(&Settings::default(), Some(64), true, None);
        assert!(dev.graded.is_some(), "the preview was not kept");
        assert_eq!(dev.render(&graded, Some(64), true, None).0.d, fresh.d);
        let mut warmer = graded.clone();
        warmer.temperature = 0.6;
        let fresh = development().render(&warmer, Some(64), true, None).0;
        assert_eq!(dev.render(&warmer, Some(64), true, None).0.d, fresh.d, "a stale kept preview");
    }

    /// Matched to its own picture with a tint graded in, a photograph gets
    /// wheels pointing the way of that tint.
    #[test]
    fn matching_a_tinted_copy_finds_the_tint() {
        let mut dev = development();
        let plain = Settings::default();
        let mut tinted = plain.clone();
        tinted.wheels.shadows = crate::wheels::Wheel { hue: 220.0, amount: 0.7, lum: 0.0 };
        tinted.wheels.highlights = crate::wheels::Wheel { hue: 60.0, amount: 0.7, lum: 0.0 };
        let (reference, _) = dev.render(&tinted, Some(384), true, None);
        let found = dev.match_colour(&plain, &reference);
        let near = |a: f32, b: f32| (a - b + 540.0).rem_euclid(360.0) - 180.0;
        for (wheel, hue) in [(found.shadows, 220.0), (found.highlights, 60.0)] {
            assert!(wheel.amount > 0.3 && near(wheel.hue, hue).abs() < 35.0,
                    "wanted about {hue} degrees, found {wheel:?}");
        }
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
        let s = Settings::from_json(&json!({"clarity": 0.6, "denoise": 0.5, "vignette": 0.7, "dehaze": 0.6,
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

    /// Half the frame asked for at 160 pixels is cut from the frame at 320,
    /// and sharpened as the whole frame at 320 is.
    #[test]
    fn a_crop_does_not_change_the_scale() {
        let mut img = Image::new(640, 420);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            let (x, y) = ((i % 640) as f32, (i / 640) as f32);
            let texture = 0.05 * ((x * 0.31).sin() * (y * 0.27).cos());
            *px = [0.1 + 0.2 * x / 640.0 + texture, 0.1 + texture, 0.08 + 0.1 * y / 420.0];
        }
        let mut dev = Development::from_frame("test", img, Meta::default(), Vec::new(), None);
        let whole = Settings::default();
        let cropped = Settings::from_json(&json!({"framing": {"crop": [0.25, 0.25, 0.5, 0.5]}}));
        let (a, ra) = dev.render(&whole, Some(320), true, None);
        let (b, rb) = dev.render(&cropped, Some(160), true, None);
        assert_eq!(a.w.max(a.h), 320);
        assert_eq!(b.w.max(b.h), 160, "the crop came out {}x{}", b.w, b.h);
        assert_eq!(ra.get("sharpening")["amount"], rb.get("sharpening")["amount"]);
    }

    /// A lens chosen by hand is the one corrected for, in place of the one
    /// found from the file.
    #[test]
    fn a_chosen_lens_is_the_one_corrected_for() {
        let lens = |model: &str| crate::lensdb::LensEntry {
            maker: "Someone".into(), model: model.into(), mount: "Sony E".into(), crop: 1.534,
            aspect: 1.5, tca: Vec::new(), vign: Vec::new(),
            dist: vec![crate::lensdb::DistEntry { f: 30.0, m: "poly3".into(), k1: -0.02,
                                                   ..Default::default() }],
        };
        let db = Database {
            cameras: vec![crate::lensdb::CameraEntry { maker: "Sony".into(), model: "ILCE-5000".into(),
                                                       mount: "Sony E".into(), crop: 1.534 }],
            lenses: vec![lens("30mm F1.4 DC DN"), lens("30mm F2.8 DN")],
            mounts: None,
        };
        let meta = Meta { make: "SONY".into(), model: "ILCE-5000".into(),
                          lens_model: "E 30mm F1.4".into(), focal_mm: 30.0, aperture: 1.4,
                          ..Meta::default() };
        let mut dev = Development::from_frame("test", Image::new(96, 64), meta, Vec::new(), Some(&db));
        let found = |dev: &mut Development, s: &Settings| {
            let (_, report) = dev.render(s, Some(64), true, None);
            report.get("distortion")["lens"].as_str().unwrap_or_default().to_string()
        };
        assert_eq!(found(&mut dev, &Settings::default()), "Someone 30mm F1.4 DC DN");
        let chosen = Settings { lens: "30mm F2.8 DN".into(), ..Settings::default() };
        assert_eq!(found(&mut dev, &chosen), "Someone 30mm F2.8 DN");
        assert_eq!(dev.describe()["lens_choices"].as_array().unwrap().len(), 2);
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

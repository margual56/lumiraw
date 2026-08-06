//! The wasm surface.

use crate::develop::Development;
use crate::grade::Settings;
use crate::lensdb::Database;
use crate::output;
use serde_json::json;
use std::cell::RefCell;

thread_local! {
    static DB: RefCell<Option<Database>> = RefCell::new(None);
    static BRACKET: RefCell<Vec<crate::merge::Frame>> = RefCell::new(Vec::new());
    static DEV: RefCell<Option<Development>> = RefCell::new(None);
    static JSON: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static PIXELS: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static PIXELS_B: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static SIZE: RefCell<(u32, u32)> = RefCell::new((0, 0));
    static SIZE_B: RefCell<(u32, u32)> = RefCell::new((0, 0));
}

// -- progress out to the host ---------------------------------------------

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn host_progress(fraction: f32, code_ptr: *const u8, code_len: usize);
}

fn progress(fraction: f32, code: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        host_progress(fraction, code.as_ptr(), code.len());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (fraction, code);
    }
}

// -- memory ----------------------------------------------------------------

/// Hand JS a buffer to write into (the raw file, the database).
#[no_mangle]
pub extern "C" fn ar_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len];
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

#[no_mangle]
pub extern "C" fn ar_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() && len > 0 {
        unsafe {
            drop(Vec::from_raw_parts(ptr, len, len));
        }
    }
}

unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

fn set_json(v: serde_json::Value) {
    JSON.with(|j| *j.borrow_mut() = serde_json::to_vec(&v).unwrap_or_default());
}

fn set_error(message: &str, code: &str) -> i32 {
    set_json(json!({"error": message, "code": code}));
    -1
}

// -- readable results ------------------------------------------------------

#[no_mangle]
pub extern "C" fn ar_json_ptr() -> *const u8 {
    JSON.with(|j| j.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn ar_json_len() -> usize {
    JSON.with(|j| j.borrow().len())
}

#[no_mangle]
pub extern "C" fn ar_pixels_ptr() -> *const u8 {
    PIXELS.with(|p| p.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn ar_pixels_len() -> usize {
    PIXELS.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn ar_width() -> u32 {
    SIZE.with(|s| s.borrow().0)
}

#[no_mangle]
pub extern "C" fn ar_height() -> u32 {
    SIZE.with(|s| s.borrow().1)
}

#[no_mangle]
pub extern "C" fn ar_pixels_b_ptr() -> *const u8 {
    PIXELS_B.with(|p| p.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn ar_pixels_b_len() -> usize {
    PIXELS_B.with(|p| p.borrow().len())
}

#[no_mangle]
pub extern "C" fn ar_width_b() -> u32 {
    SIZE_B.with(|s| s.borrow().0)
}

#[no_mangle]
pub extern "C" fn ar_height_b() -> u32 {
    SIZE_B.with(|s| s.borrow().1)
}

#[no_mangle]
pub extern "C" fn ar_bytes_ptr() -> *const u8 {
    BYTES.with(|b| b.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn ar_bytes_len() -> usize {
    BYTES.with(|b| b.borrow().len())
}

// -- session ---------------------------------------------------------------

/// Load the baked lensfun database.  Optional: without it the pipeline still
/// runs, it just cannot correct the lens or read an authoritative crop factor.
#[no_mangle]
pub extern "C" fn ar_set_database(ptr: *const u8, len: usize) -> i32 {
    let bytes = unsafe { slice(ptr, len) };
    match Database::parse(bytes) {
        Ok(db) => {
            let n = db.lenses.len();
            DB.with(|d| *d.borrow_mut() = Some(db));
            set_json(json!({"lenses": n}));
            0
        }
        Err(e) => set_error(&e, "database"),
    }
}

/// Decode a raw file.  On success the description (size, capture profile,
/// notes, lens calibration) is left in the JSON buffer.
#[no_mangle]
pub extern "C" fn ar_open(name_ptr: *const u8, name_len: usize, ptr: *const u8, len: usize) -> i32 {
    let name = String::from_utf8_lossy(unsafe { slice(name_ptr, name_len) }).to_string();
    let bytes = unsafe { slice(ptr, len) };
    if bytes.is_empty() {
        return set_error("empty file", "unreadable");
    }
    progress(0.05, "decoding");
    let result = DB.with(|db| Development::open(&name, bytes, db.borrow().as_ref()));
    match result {
        Ok(dev) => {
            let mut description = dev.describe();
            if let Some(o) = description.as_object_mut() {
                o.insert("formats".into(), json!(output::FORMATS.iter()
                    .map(|f| json!({"id": f.id, "label": f.label, "ext": f.ext, "mime": f.mime}))
                    .collect::<Vec<_>>()));

            }
            DEV.with(|d| *d.borrow_mut() = Some(dev));
            set_json(description);
            progress(1.0, "done");
            0
        }
        // What kind of failure it was travels with it, so the interface can
        // tell a camera the decoder has not met from a damaged file.
        Err(e) => {
            set_json(json!({"error": e.message, "code": e.code,
                            "params": {"file": name, "make": e.make, "model": e.model}}));
            -1
        }
    }
}

fn with_dev<F, T>(f: F) -> Option<T>
where
    F: FnOnce(&mut Development) -> T,
{
    DEV.with(|d| d.borrow_mut().as_mut().map(f))
}

fn store(img: &crate::ops::Image) {
    PIXELS.with(|p| *p.borrow_mut() = output::rgba8(img));
    SIZE.with(|s| *s.borrow_mut() = (img.w as u32, img.h as u32));
}

fn store_b(img: &crate::ops::Image) {
    PIXELS_B.with(|p| *p.borrow_mut() = output::rgba8(img));
    SIZE_B.with(|s| *s.borrow_mut() = (img.w as u32, img.h as u32));
}

/// Develop a preview.  `long_edge <= 0` means full resolution; `crop = 0`
/// renders the whole framing canvas (what the framing step shows).
#[no_mangle]
pub extern "C" fn ar_render(settings_ptr: *const u8, settings_len: usize, long_edge: i32,
                            crop: i32) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let edge = if long_edge > 0 { Some(long_edge as usize) } else { None };

    let out = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f, code);
        let (img, report) = dev.render(&settings, edge, crop != 0, Some(&mut cb));
        let toggles = dev.toggles(&settings, &report);
        let eff = crate::geometry::effective_crop(dev.width(), dev.height(), &settings.framing);
        (img, report.to_json(), toggles, eff)
    });
    match out {
        Some((img, report, toggles, eff)) => {
            store(&img);
            set_json(json!({"width": img.w, "height": img.h, "report": report,
                            "toggles": toggles,
                            "crop": eff.map(|c| vec![c.0, c.1, c.2, c.3])}));
            0
        }
        None => set_error("no image open", "no_session"),
    }
}

/// A filmstrip thumbnail of a file, without opening it: the photograph being
/// developed is left alone. Pixels in the primary buffer, size in the JSON.
#[no_mangle]
pub extern "C" fn ar_thumbnail(ptr: *const u8, len: usize, long_edge: i32) -> i32 {
    let bytes = unsafe { slice(ptr, len) };
    match crate::decode::thumbnail(bytes, long_edge.max(16) as usize) {
        Ok(img) => {
            store(&img);
            set_json(json!({"width": img.w, "height": img.h}));
            0
        }
        Err(e) => set_error(&e.message, e.code),
    }
}

/// Before/after: the corrected frame in the primary buffer, the same frame
/// with every automatic correction switched off in the secondary one.
#[no_mangle]
pub extern "C" fn ar_compare(settings_ptr: *const u8, settings_len: usize, long_edge: i32) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let edge = if long_edge > 0 { Some(long_edge as usize) } else { None };

    let out = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f * 0.5, code);
        let (after, report) = dev.render(&settings, edge, true, Some(&mut cb));
        progress(0.5, "baseline");
        let before = dev.baseline(&settings, edge);
        let toggles = dev.toggles(&settings, &report);
        (after, before, report.to_json(), toggles)
    });
    match out {
        Some((after, before, report, toggles)) => {
            store(&after);
            store_b(&before);
            set_json(json!({"width": after.w, "height": after.h, "report": report,
                            "toggles": toggles}));
            progress(1.0, "done");
            0
        }
        None => set_error("no image open", "no_session"),
    }
}

/// Encode a finished image for download.  Full resolution unless `long_edge`
/// says otherwise; the bytes land in the BYTES buffer.
#[no_mangle]
pub extern "C" fn ar_export(settings_ptr: *const u8, settings_len: usize,
                            fmt_ptr: *const u8, fmt_len: usize, quality: i32,
                            long_edge: i32, now_ptr: *const u8, now_len: usize) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let fmt = String::from_utf8_lossy(unsafe { slice(fmt_ptr, fmt_len) }).to_string();
    let edge = if long_edge > 0 { Some(long_edge as usize) } else { None };
    // There is no clock in this target, so the host passes the time in.
    let now = String::from_utf8_lossy(unsafe { slice(now_ptr, now_len) }).to_string();
    let modified = if now.is_empty() { None } else { Some(now.as_str()) };

    let developed = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f * 0.8, code);
        let (img, _) = dev.render(&settings, edge, true, Some(&mut cb));
        (img, dev.exif.clone())
    });
    let (img, meta) = match developed {
        Some(pair) => pair,
        None => return set_error("no image open", "no_session"),
    };
    // When the photograph was taken, so that a bundle can date each member by
    // it rather than by the moment the zip was written.
    let captured = meta.iter()
        .find(|e| e.tag == crate::exif::TAG_DATE_TIME_ORIGINAL)
        .or_else(|| meta.iter().find(|e| e.tag == crate::exif::TAG_DATE_TIME))
        .and_then(|e| match &e.value {
            crate::exif::Value::Ascii(s) => Some(s.trim_end_matches('\0').to_string()),
            _ => None,
        });

    progress(0.85, "encoding");
    // WebP has no pure-Rust encoder we want to carry, so the worker asks the
    // browser's own canvas encoder for it; everything else is encoded here.
    if fmt == "webp" {
        // The browser encodes the picture.
        let block = output::exif_block(&img, &meta, modified);
        let n = block.len();
        BYTES.with(|b| *b.borrow_mut() = block);
        store(&img);
        set_json(json!({"width": img.w, "height": img.h, "canvas": true, "format": "webp",
                        "captured": captured, "exif": n}));
        progress(1.0, "done");
        return 0;
    }
    match output::save(&img, &fmt, quality.clamp(1, 100) as u8, &meta, modified) {
        Ok(bytes) => {
            let n = bytes.len();
            BYTES.with(|b| *b.borrow_mut() = bytes);
            let spec = output::format_spec(&fmt);
            set_json(json!({"width": img.w, "height": img.h, "bytes": n,
                            "mime": spec.mime, "ext": spec.ext, "canvas": false,
                            "captured": captured}));
            progress(1.0, "done");
            0
        }
        Err(e) => set_error(&e, "encode"),
    }
}

// -- merging a bracket -----------------------------------------------------

/// What is wrong with the bracket so far, from the metadata alone.
#[no_mangle]
pub extern "C" fn ar_merge_check() -> i32 {
    let findings = BRACKET.with(|b| crate::merge::inspect(&b.borrow()));
    set_json(json!({"findings": findings.iter().map(|f| json!({
        "code": f.code, "frames": f.frames, "value": (f.value * 100.0).round() / 100.0,
    })).collect::<Vec<_>>()}));
    0
}

/// Start a new bracket, forgetting any frames already gathered.
#[no_mangle]
pub extern "C" fn ar_merge_reset() {
    BRACKET.with(|b| b.borrow_mut().clear());
}

/// Drop one exposure from the bracket.  Without this, changing your mind
/// about a frame means decoding every other one again.
#[no_mangle]
pub extern "C" fn ar_merge_remove(index: i32) -> i32 {
    BRACKET.with(|b| {
        let mut frames = b.borrow_mut();
        if index < 0 || index as usize >= frames.len() {
            return -1;
        }
        frames.remove(index as usize);
        0
    })
}

/// Decode one exposure and add it to the bracket.
#[no_mangle]
pub extern "C" fn ar_merge_add(name_ptr: *const u8, name_len: usize, ptr: *const u8,
                               len: usize) -> i32 {
    let name = String::from_utf8_lossy(unsafe { slice(name_ptr, name_len) }).to_string();
    let bytes = unsafe { slice(ptr, len) };
    progress(0.1, "decoding");
    match crate::merge::Frame::open(&name, bytes) {
        Ok(frame) => {
            let (w, h) = (frame.linear.w, frame.linear.h);
            let exposure = frame.exposure();
            let (iso, shutter, aperture) =
                (frame.meta.iso, frame.meta.shutter_s, frame.meta.aperture);
            let comp = frame.meta.exposure_comp;
            BRACKET.with(|b| b.borrow_mut().push(frame));
            let count = BRACKET.with(|b| b.borrow().len());
            set_json(json!({"name": name, "width": w, "height": h, "count": count,
                            "iso": iso, "shutter_s": shutter, "aperture": aperture,
                            // What the dial was set to, which is not always
                            // what the camera managed to do.
                            "exposure_comp": comp,
                            "exposure": exposure}));
            progress(1.0, "done");
            0
        }
        // What kind of failure it was travels with it, so the interface can
        // tell a camera the decoder has not met from a damaged file.
        Err(e) => {
            set_json(json!({"error": e.message, "code": e.code,
                            "params": {"file": name, "make": e.make, "model": e.model}}));
            -1
        }
    }
}

/// Merge what has been gathered and make the result the frame being developed.
#[no_mangle]
pub extern "C" fn ar_merge_finish(align: i32, deghost: f32) -> i32 {
    // A negative amount asks the merge to measure one instead of being told.
    let options = crate::merge::Options {
        align: align != 0,
        deghost: if deghost < 0.0 { None } else { Some(deghost.clamp(0.0, 1.0)) },
    };
    progress(0.15, "merging");
    let merged = BRACKET.with(|b| {
        let frames = b.borrow();
        crate::merge::merge(&frames, options).map(|(img, notes)| {
            // The reference frame's metadata describes the result.
            let reference = &frames[notes.reference];
            (img, reference.meta.clone(), reference.exif.clone(), reference.name.clone(),
             notes)
        })
    });
    let (img, meta, exif, name, notes) = match merged {
        Ok(v) => v,
        Err(e) => return set_error(&e, "merge"),
    };
    progress(0.8, "describing");

    let dev = DB.with(|db| {
        crate::develop::Development::from_frame(&format!("{name} (merged)"), img, meta,
                                                exif, db.borrow().as_ref())
    });
    let mut description = dev.describe();
    if let Some(o) = description.as_object_mut() {
        o.insert("formats".into(), json!(output::FORMATS.iter()
            .map(|f| json!({"id": f.id, "label": f.label, "ext": f.ext, "mime": f.mime}))
            .collect::<Vec<_>>()));
        let ev = |v: f32| (v * 100.0).round() / 100.0;
        o.insert("merge".into(), json!({
            "frames": notes.stops.len(),
            "reference": notes.reference,
            "stops": notes.stops.iter().map(|v| ev(*v)).collect::<Vec<_>>(),
            "range_stops": ev(notes.range_stops),
            "shifts": notes.shifts.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>(),
            "ghosted": (notes.ghosted * 10000.0).round() / 10000.0,
            // What the deghosting was set to, so the interface can say so when
            // it was the merge rather than the photographer that chose it.
            "deghost_used": (notes.deghost_used * 100.0).round() / 100.0,
            "uncovered": (notes.uncovered * 10000.0).round() / 10000.0,
            // What the photographer asked for, against what the frames turned
            // out to be. These differ when the camera ran out of shutter.
            "bias": notes.bias.iter().map(|v| ev(*v)).collect::<Vec<_>>(),
            "intended_range_stops": ev(notes.intended_range_stops),
            // Everything the merge decided or found odd, for the interface to
            // put into words.
            "findings": notes.findings.iter().map(|f| json!({
                "code": f.code, "frames": f.frames, "value": ev(f.value),
            })).collect::<Vec<_>>(),
            // Where the pixels put each frame, and whether that had to be used
            // in place of metadata that could not be right.
            "measured": notes.measured.iter().map(|v| v.map(ev)).collect::<Vec<_>>(),
            "remeasured": notes.remeasured,
        }));
    }
    DEV.with(|d| *d.borrow_mut() = Some(dev));
    BRACKET.with(|b| b.borrow_mut().clear());
    set_json(description);
    progress(1.0, "done");
    0
}

/// Which build this is.
#[no_mangle]
pub extern "C" fn ar_lut_load(ptr: *const u8, len: usize) -> i32 {
    let bytes = unsafe { slice(ptr, len) };
    let text = String::from_utf8_lossy(bytes);
    match crate::lut::Lut::parse(&text) {
        // Whatever was loaded stays loaded.
        Err(why) => set_error(&why, "bad_cube"),
        Ok(lut) => {
            let (size, title) = (lut.size, lut.title.clone());
            // A cheap digest of the file, which is all the cache key needs: it
            // has to differ when the file differs, not resist an adversary.
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
            crate::lut::set(Some(lut));
            set_json(json!({"size": size, "title": title, "id": format!("{h:x}"),
                            "bytes": bytes.len()}));
            0
        }
    }
}

/// Forget the loaded table.
#[no_mangle]
pub extern "C" fn ar_lut_clear() -> i32 {
    crate::lut::set(None);
    set_json(json!({"loaded": false}));
    0
}

/// Every named grade, with its control points.
#[no_mangle]
pub extern "C" fn ar_looks() -> i32 {
    let points = |p: &[(f32, f32)]| {
        p.iter().map(|(x, y)| json!([crate::grade::round_to(*x, 4),
                                     crate::grade::round_to(*y, 4)])).collect::<Vec<_>>()
    };
    set_json(json!({"looks": crate::looks::LOOKS.iter().map(|l| json!({
        "id": l.id, "label": l.label, "description": l.description,
        "points": {"rgb": points(l.rgb), "r": points(l.r), "g": points(l.g),
                   "b": points(l.b)},
    })).collect::<Vec<_>>()}));
    0
}

/// The curves a settings object comes to, as the pipeline will evaluate them.
#[no_mangle]
pub extern "C" fn ar_curves(settings_ptr: *const u8, settings_len: usize) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let stack = &settings.curves;
    let table = |c: &crate::curve::Curve| {
        c.table().iter().map(|v| crate::grade::round_to(*v, 4)).collect::<Vec<_>>()
    };
    let counts = stack.counts();
    // Two sets, and the difference matters.
    let raw = crate::curve::edited(settings_json.get("curves"));
    set_json(json!({
        "identity": stack.is_identity(),
        "size": crate::curve::TABLE,
        "rgb": table(stack.composite()),
        "r": table(stack.channel(0)),
        "g": table(stack.channel(1)),
        "b": table(stack.channel(2)),
        "edited": {"regions": table(&raw[0]), "rgb": table(&raw[1]), "r": table(&raw[2]),
                   "g": table(&raw[3]), "b": table(&raw[4])},
        "points": {"rgb": counts[0], "r": counts[1], "g": counts[2], "b": counts[3]},
    }));
    0
}

#[no_mangle]
pub extern "C" fn ar_version() -> i32 {
    set_json(json!({"version": crate::VERSION, "software": crate::SOFTWARE}));
    0
}

/// Free the decoded frame (a new upload, or the tab going idle).
#[no_mangle]
pub extern "C" fn ar_close() {
    DEV.with(|d| *d.borrow_mut() = None);
    PIXELS.with(|p| p.borrow_mut().clear());
    PIXELS_B.with(|p| p.borrow_mut().clear());
    BYTES.with(|b| b.borrow_mut().clear());
}

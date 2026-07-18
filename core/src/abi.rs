//! The wasm surface.

use crate::develop::Development;
use crate::grade::Settings;
use crate::lensdb::Database;
use crate::output;
use crate::styles;
use serde_json::json;
use std::cell::RefCell;

thread_local! {
    static DB: RefCell<Option<Database>> = RefCell::new(None);
    static DEV: RefCell<Option<Development>> = RefCell::new(None);
    static JSON: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static PIXELS: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static PIXELS_B: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static BYTES: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static SIZE: RefCell<(u32, u32)> = RefCell::new((0, 0));
    static SIZE_B: RefCell<(u32, u32)> = RefCell::new((0, 0));
    static STYLE_BASE: RefCell<Option<crate::ops::Image>> = RefCell::new(None);
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
                o.insert("styles".into(), json!(styles::STYLES.iter()
                    .map(|s| json!({"id": s.id, "label": s.label, "description": s.description}))
                    .collect::<Vec<_>>()));
            }
            DEV.with(|d| *d.borrow_mut() = Some(dev));
            set_json(description);
            progress(1.0, "done");
            0
        }
        Err(e) => set_error(&e, "unreadable"),
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
                            crop: i32, style_ptr: *const u8, style_len: usize) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let style = String::from_utf8_lossy(unsafe { slice(style_ptr, style_len) }).to_string();
    let edge = if long_edge > 0 { Some(long_edge as usize) } else { None };

    let out = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f, code);
        let (img, report) = dev.render(&settings, &style, edge, crop != 0, Some(&mut cb));
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
        let (after, report) = dev.render(&settings, "original", edge, true, Some(&mut cb));
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

/// Render the shared base for the looks grid once; `ar_style_tile` then costs
/// only the look itself.
#[no_mangle]
pub extern "C" fn ar_styles_prepare(settings_ptr: *const u8, settings_len: usize, size: i32) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let edge = (size.max(200).min(1400)) as usize;
    let out = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f * 0.5, code);
        let (img, _) = dev.render(&settings, "original", Some(edge), true, Some(&mut cb));
        img
    });
    match out {
        Some(img) => {
            STYLE_BASE.with(|b| *b.borrow_mut() = Some(img));
            set_json(json!({"count": styles::STYLES.len(),
                            "styles": styles::STYLES.iter().map(|s| json!({
                                "id": s.id, "label": s.label, "description": s.description}))
                                .collect::<Vec<_>>()}));
            styles::STYLES.len() as i32
        }
        None => set_error("no image open", "no_session"),
    }
}

#[no_mangle]
pub extern "C" fn ar_style_tile(index: i32) -> i32 {
    let i = index.max(0) as usize;
    if i >= styles::STYLES.len() {
        return set_error("no such style", "range");
    }
    let s = &styles::STYLES[i];
    let done = STYLE_BASE.with(|b| {
        b.borrow().as_ref().map(|base| {
            let img = styles::apply(base, s.id);
            store(&img);
            (img.w, img.h)
        })
    });
    match done {
        Some((w, h)) => {
            progress(0.5 + 0.5 * (i as f32 + 1.0) / styles::STYLES.len() as f32, "style");
            set_json(json!({"id": s.id, "label": s.label, "description": s.description,
                            "width": w, "height": h}));
            0
        }
        None => set_error("looks not prepared", "no_session"),
    }
}

/// Encode a finished image for download.  Full resolution unless `long_edge`
/// says otherwise; the bytes land in the BYTES buffer.
#[no_mangle]
pub extern "C" fn ar_export(settings_ptr: *const u8, settings_len: usize, style_ptr: *const u8,
                            style_len: usize, fmt_ptr: *const u8, fmt_len: usize, quality: i32,
                            long_edge: i32) -> i32 {
    let settings_json: serde_json::Value =
        serde_json::from_slice(unsafe { slice(settings_ptr, settings_len) }).unwrap_or(json!({}));
    let settings = Settings::from_json(&settings_json);
    let style = String::from_utf8_lossy(unsafe { slice(style_ptr, style_len) }).to_string();
    let fmt = String::from_utf8_lossy(unsafe { slice(fmt_ptr, fmt_len) }).to_string();
    let edge = if long_edge > 0 { Some(long_edge as usize) } else { None };

    let developed = with_dev(|dev| {
        let mut cb = |f: f32, code: &str| progress(f * 0.8, code);
        let (img, _) = dev.render(&settings, &style, edge, true, Some(&mut cb));
        (img, dev.exif.clone())
    });
    let (img, meta) = match developed {
        Some(pair) => pair,
        None => return set_error("no image open", "no_session"),
    };
    progress(0.85, "encoding");
    // WebP has no pure-Rust encoder we want to carry, so the worker asks the
    // browser's own canvas encoder for it; everything else is encoded here.
    if fmt == "webp" {
        // The browser encodes the picture.
        let block = output::exif_block(&img, &meta);
        let n = block.len();
        BYTES.with(|b| *b.borrow_mut() = block);
        store(&img);
        set_json(json!({"width": img.w, "height": img.h, "canvas": true, "format": "webp",
                        "exif": n}));
        progress(1.0, "done");
        return 0;
    }
    match output::save(&img, &fmt, quality.clamp(1, 100) as u8, &meta) {
        Ok(bytes) => {
            let n = bytes.len();
            BYTES.with(|b| *b.borrow_mut() = bytes);
            let spec = output::format_spec(&fmt);
            set_json(json!({"width": img.w, "height": img.h, "bytes": n,
                            "mime": spec.mime, "ext": spec.ext, "canvas": false}));
            progress(1.0, "done");
            0
        }
        Err(e) => set_error(&e, "encode"),
    }
}

/// Which build this is. Read once at start-up so the interface can show it
/// and so every exported file can be stamped with the same string.
#[no_mangle]
pub extern "C" fn ar_version() -> i32 {
    set_json(json!({"version": crate::VERSION, "software": crate::SOFTWARE}));
    0
}

/// Free the decoded frame (a new upload, or the tab going idle).
#[no_mangle]
pub extern "C" fn ar_close() {
    DEV.with(|d| *d.borrow_mut() = None);
    STYLE_BASE.with(|b| *b.borrow_mut() = None);
    PIXELS.with(|p| p.borrow_mut().clear());
    PIXELS_B.with(|p| p.borrow_mut().clear());
    BYTES.with(|b| b.borrow_mut().clear());
}

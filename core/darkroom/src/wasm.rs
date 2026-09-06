//! The API as a wasm module, for the web app's worker.

use crate::{Bracket, Edit, Encoded, Export, Lenses, MergeOptions, Photo, Size};
use serde_json::json;
use std::cell::RefCell;

thread_local! {
    static LENSES: RefCell<Option<Lenses>> = RefCell::new(None);
    static BRACKET: RefCell<Bracket> = RefCell::new(Bracket::new());
    static PHOTO: RefCell<Option<Photo>> = RefCell::new(None);
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

/// The settings JSON the page passed, as it was sent; anything unreadable is
/// an empty object, which means the defaults.
fn json_at(ptr: *const u8, len: usize) -> serde_json::Value {
    serde_json::from_slice(unsafe { slice(ptr, len) }).unwrap_or(json!({}))
}

fn edit_at(ptr: *const u8, len: usize) -> Edit {
    Edit::from_json(&json_at(ptr, len))
}

/// A string the page passed, as UTF-8 (lossily: a file name is not worth
/// failing over).
fn text_at(ptr: *const u8, len: usize) -> String {
    String::from_utf8_lossy(unsafe { slice(ptr, len) }).to_string()
}

/// The size a render was asked for; zero or less means every pixel.
fn size_of(long_edge: i32) -> Size {
    if long_edge > 0 { Size::LongEdge(long_edge as usize) } else { Size::Full }
}

fn set_json(v: serde_json::Value) {
    JSON.with(|j| *j.borrow_mut() = serde_json::to_vec(&v).unwrap_or_default());
}

fn set_error(message: &str, code: &str) -> i32 {
    set_json(json!({"error": message, "code": code}));
    -1
}

/// A file that could not be read, with what kind of failure it was, so the
/// interface can tell a camera the decoder has not met from a damaged file.
fn set_decode_error(name: &str, e: crate::DecodeError) -> i32 {
    set_json(json!({"error": e.message, "code": e.code,
                    "params": {"file": name, "make": e.make, "model": e.model}}));
    -1
}

fn store(img: &kit::Image) {
    PIXELS.with(|p| *p.borrow_mut() = crate::rgba8(img));
    SIZE.with(|s| *s.borrow_mut() = (img.w as u32, img.h as u32));
}

fn store_b(img: &kit::Image) {
    PIXELS_B.with(|p| *p.borrow_mut() = crate::rgba8(img));
    SIZE_B.with(|s| *s.borrow_mut() = (img.w as u32, img.h as u32));
}

fn with_photo<T>(f: impl FnOnce(&mut Photo) -> T) -> Option<T> {
    PHOTO.with(|p| p.borrow_mut().as_mut().map(f))
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

// -- the calls ---------------------------------------------------------------

/// A photograph's description, plus the lens mount whose calibrations it is
/// still waiting for (`lens_mount_needed`, null when none).
fn describe(photo: &Photo) -> serde_json::Value {
    let missing = LENSES.with(|l| l.borrow().as_ref().and_then(|l| photo.missing_mount(l)));
    kit::with(photo.describe(), json!({"lens_mount_needed": missing}))
}

/// After the database changes: the open photograph looks its lens up again,
/// and its description is what the call answers with.
fn relens() -> i32 {
    let described = LENSES.with(|l| with_photo(|photo| {
        if let Some(lenses) = l.borrow().as_ref() {
            photo.use_lenses(lenses);
        }
        describe(photo)
    }));
    let count = LENSES.with(|l| l.borrow().as_ref().map_or(0, |l| l.len()));
    set_json(kit::with(described.unwrap_or(json!({})), json!({"lenses": count})));
    0
}

/// Replace the lens database: the whole of it, or the cameras alone to start
/// from (see `ar_add_lenses`).
#[no_mangle]
pub extern "C" fn ar_set_database(ptr: *const u8, len: usize) -> i32 {
    match Lenses::parse(unsafe { slice(ptr, len) }) {
        Ok(lenses) => {
            LENSES.with(|l| *l.borrow_mut() = Some(lenses));
            relens()
        }
        Err(e) => set_error(&e, "database"),
    }
}

/// Add one mount's lenses to the database, and find the open photograph's
/// lens among them.
#[no_mangle]
pub extern "C" fn ar_add_lenses(ptr: *const u8, len: usize) -> i32 {
    match Lenses::parse(unsafe { slice(ptr, len) }) {
        Ok(more) => {
            LENSES.with(|l| {
                let mut slot = l.borrow_mut();
                if let Some(lenses) = slot.as_mut() {
                    lenses.extend(more);
                } else {
                    *slot = Some(more);
                }
            });
            relens()
        }
        Err(e) => set_error(&e, "database"),
    }
}

/// Open a raw file; its description is left in the JSON buffer.
#[no_mangle]
pub extern "C" fn ar_open(name_ptr: *const u8, name_len: usize, ptr: *const u8, len: usize) -> i32 {
    let name = text_at(name_ptr, name_len);
    progress(0.05, "decoding");
    match LENSES.with(|l| crate::open(&name, unsafe { slice(ptr, len) }, l.borrow().as_ref())) {
        Ok(photo) => {
            set_json(describe(&photo));
            PHOTO.with(|p| *p.borrow_mut() = Some(photo));
            progress(1.0, "done");
            0
        }
        Err(e) => set_decode_error(&name, e),
    }
}

/// Develop; `long_edge <= 0` means every pixel, `crop = 0` the whole framing
/// canvas.
#[no_mangle]
pub extern "C" fn ar_render(settings_ptr: *const u8, settings_len: usize, long_edge: i32,
                            crop: i32) -> i32 {
    let edit = edit_at(settings_ptr, settings_len);
    let out = with_photo(|photo| {
        let mut cb = |f: f32, code: &str| progress(f, code);
        if crop != 0 {
            photo.develop(&edit, size_of(long_edge), Some(&mut cb))
        } else {
            photo.develop_canvas(&edit, size_of(long_edge), Some(&mut cb))
        }
    });
    match out {
        Some(d) => {
            store(&d.image);
            set_json(json!({"width": d.image.w, "height": d.image.h, "report": d.report.to_json(),
                            "toggles": d.toggles,
                            "crop": d.crop.map(|c| vec![c.0, c.1, c.2, c.3])}));
            0
        }
        None => set_error("no image open", "no_session"),
    }
}

/// A filmstrip thumbnail of a file, leaving the open photograph alone.
#[no_mangle]
pub extern "C" fn ar_thumbnail(ptr: *const u8, len: usize, long_edge: i32) -> i32 {
    match crate::thumbnail(unsafe { slice(ptr, len) }, long_edge.max(16) as usize) {
        Ok(img) => {
            store(&img);
            set_json(json!({"width": img.w, "height": img.h}));
            0
        }
        Err(e) => set_error(&e.message, e.code),
    }
}

/// Before/after: the developed picture in the primary buffer, the same with
/// every automatic correction off in the secondary one.
#[no_mangle]
pub extern "C" fn ar_compare(settings_ptr: *const u8, settings_len: usize, long_edge: i32) -> i32 {
    let edit = edit_at(settings_ptr, settings_len);
    let size = size_of(long_edge);
    let out = with_photo(|photo| {
        let mut cb = |f: f32, code: &str| progress(f * 0.5, code);
        let after = photo.develop(&edit, size, Some(&mut cb));
        progress(0.5, "baseline");
        (after, photo.before(&edit, size))
    });
    match out {
        Some((after, before)) => {
            store(&after.image);
            store_b(&before);
            set_json(json!({"width": after.image.w, "height": after.image.h,
                            "report": after.report.to_json(), "toggles": after.toggles}));
            progress(1.0, "done");
            0
        }
        None => set_error("no image open", "no_session"),
    }
}

/// Export: the file in the bytes buffer, or, for WebP, the picture in the pixel
/// buffer and its EXIF block in the bytes buffer for the worker to splice in
/// after the browser encodes it.
#[no_mangle]
pub extern "C" fn ar_export(settings_ptr: *const u8, settings_len: usize,
                            fmt_ptr: *const u8, fmt_len: usize, quality: i32,
                            long_edge: i32, now_ptr: *const u8, now_len: usize) -> i32 {
    let edit = edit_at(settings_ptr, settings_len);
    let export = Export {
        format: text_at(fmt_ptr, fmt_len),
        quality: quality.clamp(1, 100) as u8,
        long_edge: if long_edge > 0 { Some(long_edge as usize) } else { None },
        modified: Some(text_at(now_ptr, now_len)),
    };
    let out = with_photo(|photo| {
        let mut cb = |f: f32, code: &str| progress(f, code);
        photo.export(&edit, &export, Some(&mut cb))
    });
    let exported = match out {
        None => return set_error("no image open", "no_session"),
        Some(Err(e)) => return set_error(&e, "encode"),
        Some(Ok(x)) => x,
    };
    let (w, h, captured) = (exported.image.w, exported.image.h, exported.captured);
    match exported.encoded {
        Encoded::Canvas { exif } => {
            let n = exif.len();
            BYTES.with(|b| *b.borrow_mut() = exif);
            store(&exported.image);
            set_json(json!({"width": w, "height": h, "canvas": true, "format": "webp",
                            "captured": captured, "exif": n}));
        }
        Encoded::File { bytes, mime, ext } => {
            let n = bytes.len();
            BYTES.with(|b| *b.borrow_mut() = bytes);
            set_json(json!({"width": w, "height": h, "bytes": n, "mime": mime, "ext": ext,
                            "canvas": false, "captured": captured}));
        }
    }
    0
}

// -- a bracket -------------------------------------------------------------

#[no_mangle]
pub extern "C" fn ar_merge_check() -> i32 {
    let findings = BRACKET.with(|b| b.borrow().check());
    set_json(json!({"findings": findings.iter().map(crate::finding_json).collect::<Vec<_>>()}));
    0
}

#[no_mangle]
pub extern "C" fn ar_merge_reset() {
    BRACKET.with(|b| b.borrow_mut().clear());
}

#[no_mangle]
pub extern "C" fn ar_merge_remove(index: i32) -> i32 {
    let removed = index >= 0 && BRACKET.with(|b| b.borrow_mut().remove(index as usize));
    if removed { 0 } else { -1 }
}

#[no_mangle]
pub extern "C" fn ar_merge_add(name_ptr: *const u8, name_len: usize, ptr: *const u8,
                               len: usize) -> i32 {
    let name = text_at(name_ptr, name_len);
    progress(0.1, "decoding");
    let added = BRACKET.with(|b| b.borrow_mut().add(&name, unsafe { slice(ptr, len) }));
    match added {
        Ok(f) => {
            let count = BRACKET.with(|b| b.borrow().len());
            set_json(json!({"name": f.name, "width": f.width, "height": f.height, "count": count,
                            "iso": f.iso, "shutter_s": f.shutter_s, "aperture": f.aperture,
                            "exposure_comp": f.exposure_comp, "exposure": f.exposure}));
            progress(1.0, "done");
            0
        }
        Err(e) => set_decode_error(&name, e),
    }
}

/// Merge the bracket; the result becomes the photograph being developed.
/// A negative `deghost` asks the merge to measure its own.
#[no_mangle]
pub extern "C" fn ar_merge_finish(align: i32, deghost: f32) -> i32 {
    let options = MergeOptions {
        align: align != 0,
        deghost: if deghost < 0.0 { None } else { Some(deghost.clamp(0.0, 1.0)) },
    };
    let merged = LENSES.with(|l| BRACKET.with(|b| {
        let mut cb = |f: f32, code: &str| progress(f, code);
        b.borrow_mut().merge(options, l.borrow().as_ref(), Some(&mut cb))
    }));
    match merged {
        Ok((photo, notes)) => {
            set_json(kit::with(describe(&photo), json!({"merge": crate::notes_json(&notes)})));
            PHOTO.with(|p| *p.borrow_mut() = Some(photo));
            0
        }
        Err(e) => set_error(&e, "merge"),
    }
}

// -- grades ----------------------------------------------------------------

/// Read a `.cube` file into the module. Nothing leaves the tab.
#[no_mangle]
pub extern "C" fn ar_lut_load(ptr: *const u8, len: usize) -> i32 {
    match crate::load_cube(unsafe { slice(ptr, len) }) {
        Ok(c) => {
            set_json(json!({"size": c.size, "title": c.title, "id": c.id, "bytes": len}));
            0
        }
        Err(why) => set_error(&why, "bad_cube"),
    }
}

#[no_mangle]
pub extern "C" fn ar_lut_clear() -> i32 {
    crate::clear_cube();
    set_json(json!({"loaded": false}));
    0
}

#[no_mangle]
pub extern "C" fn ar_looks() -> i32 {
    set_json(crate::looks());
    0
}

#[no_mangle]
pub extern "C" fn ar_curves(settings_ptr: *const u8, settings_len: usize) -> i32 {
    set_json(crate::curves(&json_at(settings_ptr, settings_len)));
    0
}

#[no_mangle]
pub extern "C" fn ar_version() -> i32 {
    set_json(json!({"version": crate::VERSION, "software": crate::SOFTWARE}));
    0
}

/// Free the open photograph (a new upload, or the tab going idle).
#[no_mangle]
pub extern "C" fn ar_close() {
    PHOTO.with(|p| *p.borrow_mut() = None);
    PIXELS.with(|p| p.borrow_mut().clear());
    PIXELS_B.with(|p| p.borrow_mut().clear());
    BYTES.with(|b| b.borrow_mut().clear());
}

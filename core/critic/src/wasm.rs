//! The critic as a wasm module, for the web app's scoring workers
//! (`web/src/lib/wasm/critic-worker.js`).
//!
//! Two networks can be held at once, by slot: 0 aesthetic, 1 technical. A
//! score is written to a fixed buffer: mean, std, then the pooled features.
//! The face models are graphs, in slots of their own: 0 the detector, 1 the
//! landmarks.

use crate::graph::Graph;
use crate::Net;
use std::cell::RefCell;

thread_local! {
    static NETS: RefCell<[Option<Net>; 2]> = const { RefCell::new([None, None]) };
    static GRAPHS: RefCell<[Option<Graph>; 2]> = const { RefCell::new([None, None]) };
    static OUT: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
    static TEXT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn cr_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len];
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

#[no_mangle]
pub extern "C" fn cr_free(ptr: *mut u8, len: usize) {
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

fn set_text(text: &str) {
    TEXT.with(|t| *t.borrow_mut() = text.as_bytes().to_vec());
}

/// The version, or the last error, as UTF-8.
#[no_mangle]
pub extern "C" fn cr_text_ptr() -> *const u8 {
    TEXT.with(|t| t.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn cr_text_len() -> usize {
    TEXT.with(|t| t.borrow().len())
}

#[no_mangle]
pub extern "C" fn cr_version() {
    set_text(crate::SOFTWARE);
}

/// Read a weights file into a slot.
#[no_mangle]
pub extern "C" fn cr_load(slot: u32, ptr: *const u8, len: usize) -> i32 {
    let slot = slot as usize;
    if slot >= 2 {
        set_text("no such slot");
        return -1;
    }
    match Net::parse(unsafe { slice(ptr, len) }) {
        Ok(net) => {
            NETS.with(|n| n.borrow_mut()[slot] = Some(net));
            0
        }
        Err(e) => {
            set_text(&e);
            -1
        }
    }
}

/// Score an RGBA picture with the network in `slot`.
#[no_mangle]
pub extern "C" fn cr_score(slot: u32, ptr: *const u8, width: u32, height: u32) -> i32 {
    let (w, h) = (width as usize, height as usize);
    let rgba = unsafe { slice(ptr, w * h * 4) };
    let score = NETS.with(|n| n.borrow().get(slot as usize)
        .and_then(|n| n.as_ref()).map(|net| net.score(rgba, w, h)));
    match score {
        Some(s) => {
            OUT.with(|o| {
                let mut o = o.borrow_mut();
                o.clear();
                o.extend([s.mean, s.std]);
                o.extend(s.feature);
            });
            0
        }
        None => {
            set_text("that network is not loaded");
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn cr_out_ptr() -> *const f32 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn cr_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

/// Read a face model into a slot: 0 the detector, 1 the landmarks.
#[no_mangle]
pub extern "C" fn cr_load_graph(slot: u32, ptr: *const u8, len: usize) -> i32 {
    let slot = slot as usize;
    if slot >= 2 {
        set_text("no such slot");
        return -1;
    }
    match Graph::parse(unsafe { slice(ptr, len) }) {
        Ok(graph) => {
            GRAPHS.with(|g| g.borrow_mut()[slot] = Some(graph));
            0
        }
        Err(e) => {
            set_text(&e);
            -1
        }
    }
}

/// The faces in an RGBA picture, largest first, written to the out buffer as
/// three values each: each eye's openness and the face's sharpness, each -1
/// when the face was too small, or not face enough, to look into. Returns how
/// many, or -1 without a detector.
#[no_mangle]
pub extern "C" fn cr_faces(ptr: *const u8, width: u32, height: u32) -> i32 {
    let (w, h) = (width as usize, height as usize);
    let rgba = unsafe { slice(ptr, w * h * 4) };
    let faces = GRAPHS.with(|g| {
        let g = g.borrow();
        g[0].as_ref().map(|detector| crate::faces::find(detector, g[1].as_ref(), rgba, w, h))
    });
    let Some(faces) = faces else {
        set_text("the face detector is not loaded");
        return -1;
    };
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        o.clear();
        for f in &faces {
            let eyes = f.eyes.unwrap_or([-1.0, -1.0]);
            o.extend([eyes[0], eyes[1], f.sharpness.unwrap_or(-1.0)]);
        }
    });
    faces.len() as i32
}

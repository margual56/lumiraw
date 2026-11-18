//! Synthetic scenes for the pipeline tests, and ways of reading what came out.
//!
//! A scene is what the decoder hands the pipeline: linear sRGB, balanced for
//! the camera's white, with the sensor clipping at 1.0.
#![allow(dead_code)]

use kit::Image;
use lumiraw_core::decode::Meta;
use lumiraw_core::develop::Development;
use lumiraw_core::grade::{Preset, Settings};

/// How far under the display's middle grey a normally metered raw records
/// it: the headroom the camera keeps for highlights, about two stops
/// (measured against cameras' own JPEGs of the same frames). A fact about cameras, written out here rather than taken from the
/// pipeline's own guess at it, so that the grey card can catch that guess
/// being wrong.
pub const HEADROOM_STOPS: f32 = 2.0;

/// Middle grey as a normally metered raw records it.
pub fn grey() -> f32 {
    Preset::default().target_key / 2f32.powf(HEADROOM_STOPS)
}

/// The colour patches the scenes are built from, in scene light.
pub const ORANGE: [f32; 3] = [0.9, 0.35, 0.05];
pub const SKY: [f32; 3] = [0.30, 0.50, 0.90];
pub const FOLIAGE: [f32; 3] = [0.25, 0.45, 0.10];
pub const SKIN: [f32; 3] = [0.80, 0.55, 0.42];
pub const RED: [f32; 3] = [0.80, 0.08, 0.08];
pub const PATCHES: [[f32; 3]; 5] = [ORANGE, SKY, FOLIAGE, SKIN, RED];

/// A little texture, so the estimators have percentiles to bite on and the
/// tone map an edge or two: ±`amount` around 1, the same on every channel.
pub fn grain(i: usize, amount: f32) -> f32 {
    1.0 + amount * ((i.wrapping_mul(2654435761) >> 7) % 1000) as f32 / 1000.0 - amount / 2.0
}

/// A frame filled by a function of the pixel's position (0..1 across and down)
/// and its index.
pub fn scene(w: usize, h: usize, f: impl Fn(f32, f32, usize) -> [f32; 3]) -> Image {
    let mut img = Image::new(w, h);
    for (i, px) in img.px_mut().iter_mut().enumerate() {
        let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
        *px = f(x, y, i);
    }
    img
}

/// A development of a scene, with what the file says about it.
pub fn development(img: Image, meta: Meta) -> Development {
    Development::from_frame("scene", img, meta, Vec::new(), None)
}

/// Develop a scene with the default (fully automatic) settings, as a preview.
pub fn develop(img: Image) -> Image {
    develop_meta(img, Meta::default())
}

pub fn develop_meta(img: Image, meta: Meta) -> Image {
    development(img, meta).render(&Settings::default(), Some(600), true, None).0
}

/// The middle row of an image.
pub fn middle_row(img: &Image) -> &[[f32; 3]] {
    &img.px()[img.h / 2 * img.w..][..img.w]
}

/// Whether the luminance of a run of pixels never falls by more than `slack`
/// (a fraction) going along it.
pub fn keeps_order(row: &[[f32; 3]], slack: f32) -> Result<(), String> {
    let y: Vec<f32> = row.iter().map(|p| kit::luminance_px(p)).collect();
    for x in 1..y.len() {
        if y[x] < y[x - 1] * (1.0 - slack) {
            return Err(format!("turned back at {x}: {} after {}", y[x], y[x - 1]));
        }
    }
    Ok(())
}

/// The mean of a rectangle (fractions of the frame), display-linear.
pub fn mean(img: &Image, x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 3] {
    let at = |f: f32, n: usize| ((f * n as f32) as usize).min(n - 1);
    let (ax, ay) = (at(x0, img.w), at(y0, img.h));
    let (bx, by) = (at(x1, img.w).max(ax + 1), at(y1, img.h).max(ay + 1));
    let piece = img.crop(ax, ay, bx - ax, by - ay);
    [0, 1, 2].map(|c| kit::mean(&piece.plane(c).d))
}

/// 8-bit sRGB code value of a display-linear value.
pub fn code(v: f32) -> f32 {
    kit::srgb_encode_scalar(v) * 255.0
}

/// Oklab lightness, chroma and hue (degrees) of a display-linear colour.
pub fn lch(px: [f32; 3]) -> (f32, f32, f32) {
    let (l, a, b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
    (l, a.hypot(b), b.atan2(a).to_degrees())
}

/// The difference between two hues, in degrees, the short way round.
pub fn hue_gap(a: f32, b: f32) -> f32 {
    (a - b + 540.0).rem_euclid(360.0) - 180.0
}

/// The share of pixels for which `f` holds.
pub fn share(px: &[[f32; 3]], f: impl Fn(&[f32; 3]) -> bool) -> f32 {
    px.iter().filter(|p| f(p)).count() as f32 / px.len().max(1) as f32
}

/// The share of pixels with a channel at the top of the 8-bit range.
pub fn clipped(img: &Image) -> f32 {
    share(img.px(), |p| p.iter().any(|v| code(*v) >= 254.5))
}

/// The share of pixels whose luminance codes below 8: crushed to black.
pub fn crushed(img: &Image) -> f32 {
    share(img.px(), |p| code(kit::luminance_px(p)) < 8.0)
}

/// The share of pixels whose luminance codes at 250 or above.
pub fn near_white(img: &Image) -> f32 {
    share(img.px(), |p| code(kit::luminance_px(p)) >= 250.0)
}

/// The median luminance as an 8-bit code.
pub fn median_code(img: &Image) -> f32 {
    code(kit::median(&kit::luminance(img).d))
}

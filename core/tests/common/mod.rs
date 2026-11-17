//! Synthetic scenes for the pipeline tests, and ways of reading what came out.
//!
//! A scene is what the decoder hands the pipeline: linear sRGB, balanced for
//! the camera's white, with the sensor clipping at 1.0. A normally metered raw
//! puts middle grey about two and a half stops under the display's target,
//! which is `GREY` here.
#![allow(dead_code)]

use kit::Image;
use lumiraw_core::decode::Meta;
use lumiraw_core::develop::Development;
use lumiraw_core::grade::Settings;

/// Middle grey as a normally metered raw records it.
pub const GREY: f32 = 0.13 / 5.66;

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

/// Develop a scene with the default (fully automatic) settings, as a preview.
pub fn develop(img: Image) -> Image {
    develop_with(img, Meta::default(), &Settings::default(), Some(600))
}

pub fn develop_with(img: Image, meta: Meta, settings: &Settings, edge: Option<usize>) -> Image {
    let mut dev = Development::from_frame("scene", img, meta, Vec::new(), None);
    dev.render(settings, edge, true, None).0
}

/// The same position in two frames of different sizes.
pub fn at(img: &Image, x: f32, y: f32) -> [f32; 3] {
    let (px, py) = ((x * img.w as f32) as usize, (y * img.h as f32) as usize);
    img.px()[py.min(img.h - 1) * img.w + px.min(img.w - 1)]
}

/// The mean of a rectangle (fractions of the frame), display-linear.
pub fn mean(img: &Image, x0: f32, y0: f32, x1: f32, y1: f32) -> [f32; 3] {
    let (ax, ay) = ((x0 * img.w as f32) as usize, (y0 * img.h as f32) as usize);
    let (bx, by) = (((x1 * img.w as f32) as usize).max(ax + 1), ((y1 * img.h as f32) as usize).max(ay + 1));
    let mut s = [0f64; 3];
    for y in ay..by.min(img.h) {
        for x in ax..bx.min(img.w) {
            let p = img.px()[y * img.w + x];
            for c in 0..3 {
                s[c] += p[c] as f64;
            }
        }
    }
    let n = ((by.min(img.h) - ay) * (bx.min(img.w) - ax)) as f64;
    s.map(|v| (v / n) as f32)
}

/// 8-bit sRGB code value of a display-linear value.
pub fn code(v: f32) -> f32 {
    kit::srgb_encode_scalar(v.clamp(0.0, 1.0)) * 255.0
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

/// The share of pixels with a channel at the top of the 8-bit range.
pub fn clipped(img: &Image) -> f32 {
    img.px().iter().filter(|p| p.iter().any(|v| code(*v) >= 254.5)).count() as f32
        / img.px().len() as f32
}

/// The share of pixels whose luminance codes below 8: crushed to black.
pub fn crushed(img: &Image) -> f32 {
    img.px().iter().filter(|p| code(kit::luminance_px(*p)) < 8.0).count() as f32
        / img.px().len() as f32
}

/// The median luminance as an 8-bit code.
pub fn median_code(img: &Image) -> f32 {
    let y: Vec<f32> = img.px().iter().map(|p| code(kit::luminance_px(p))).collect();
    kit::percentile(&y, 50.0)
}

//! Colour: the sRGB transfer curve, luminance, and Oklab (Björn Ottosson),
//! whose conversions take and give *linear* sRGB.

use crate::image::{Image, Plane};
use crate::math::cbrt;
use crate::matrix::Mat3;

/// sRGB (D65) primaries to XYZ, the same constant dcraw uses.
pub const SRGB_TO_XYZ: Mat3 = crate::matrix::Matrix3([
    [0.412453, 0.357580, 0.180423],
    [0.212671, 0.715160, 0.072169],
    [0.019334, 0.119193, 0.950227],
]);

/// Linear sRGB to Bradford's sharpened cone responses: `SRGB_TO_XYZ`, then the
/// Bradford matrix, multiplied out (a test holds it to the product).
pub const SRGB_TO_BRADFORD: Mat3 = crate::matrix::Matrix3([
    [0.422722, 0.491351, 0.027356],
    [0.055699, 0.961545, 0.023182],
    [0.021383, 0.087643, 0.980429],
]);

/// The change of light that takes `white` to neutral, as a von Kries scaling of
/// Bradford cone responses.
pub fn bradford_adaptation(white: [f32; 3]) -> Mat3 {
    let m = &SRGB_TO_BRADFORD.0;
    let lms_white: Vec<f32> = (0..3).map(|i| (0..3).map(|j| m[i][j] * white[j]).sum()).collect();
    let lms_grey: Vec<f32> = (0..3).map(|i| m[i].iter().sum()).collect();
    let mut scale = [[0f32; 3]; 3];
    for i in 0..3 {
        scale[i][i] = lms_grey[i] / lms_white[i].max(1e-6);
    }
    SRGB_TO_BRADFORD.inverse(0.0).expect("the Bradford matrix is invertible")
        .mul(&crate::matrix::Matrix3(scale).mul(&SRGB_TO_BRADFORD))
}

// -------------------------------------------------------------------------
// transfer functions
// -------------------------------------------------------------------------

/// The sRGB curves, exactly, for building the tables below and for checking
/// them against.
pub fn srgb_encode_exact(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_decode_exact(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// Entries in each transfer table.
const TRANSFER_STEPS: usize = 4096;

/// The encode curve sampled at the square root of its input.
fn encode_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| (0..=TRANSFER_STEPS + 1).map(|i| {
        let s = (i as f32 / TRANSFER_STEPS as f32).min(1.0);
        srgb_encode_exact(s * s)
    }).collect())
}

fn decode_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| (0..=TRANSFER_STEPS + 1).map(|i| {
        srgb_decode_exact((i as f32 / TRANSFER_STEPS as f32).min(1.0))
    }).collect())
}

#[inline]
fn lerp_table(table: &[f32], t: f32) -> f32 {
    let f = t * TRANSFER_STEPS as f32;
    let i = f as usize;
    let k = f - i as f32;
    table[i] + (table[i + 1] - table[i]) * k
}

#[inline]
pub fn srgb_encode_scalar(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 {
        x * 12.92
    } else {
        lerp_table(encode_table(), x.sqrt())
    }
}

#[inline]
pub fn srgb_decode_scalar(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.04045 {
        x / 12.92
    } else {
        lerp_table(decode_table(), x)
    }
}

/// A frame in Oklab, one plane per coordinate.
pub struct Lab {
    pub l: Plane,
    pub a: Plane,
    pub b: Plane,
}

impl Lab {
    pub fn new(w: usize, h: usize) -> Lab {
        Lab { l: Plane::new(w, h), a: Plane::new(w, h), b: Plane::new(w, h) }
    }

    /// Linear sRGB to Oklab, pixel by pixel.
    pub fn from_rgb(img: &Image) -> Lab {
        let mut lab = Lab::new(img.w, img.h);
        for (i, px) in img.px().iter().enumerate() {
            let (l, a, b) = rgb_to_oklab_px(px[0], px[1], px[2]);
            lab.l.d[i] = l;
            lab.a.d[i] = a;
            lab.b.d[i] = b;
        }
        lab
    }
}

/// Rec.709 luminance of linear RGB.
pub fn luminance(img: &Image) -> Plane {
    let mut p = Plane::new(img.w, img.h);
    for (y, px) in p.d.iter_mut().zip(img.px()) {
        *y = luminance_px(px);
    }
    p
}

#[inline]
pub fn luminance_px(px: &[f32]) -> f32 {
    0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]
}

// -------------------------------------------------------------------------
// Oklab (Björn Ottosson).  Input/output is *linear* sRGB.
// -------------------------------------------------------------------------

#[inline]
pub fn rgb_to_oklab_px(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
    let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
    let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
    let l_ = cbrt(l);
    let m_ = cbrt(m);
    let s_ = cbrt(s);
    (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )
}

#[inline]
pub fn oklab_to_rgb_px(lightness: f32, a: f32, b: f32) -> (f32, f32, f32) {
    let l_ = lightness + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = lightness - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = lightness - 0.0894841775 * a - 1.2914855480 * b;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    (
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    )
}

/// The gamut knee, in the shape of ACES's reference gamut compression.
pub const GAMUT_KNEE: f32 = 0.9;
pub const GAMUT_LIMIT: f32 = 1.4;
pub const GAMUT_POWER: f32 = 1.2;

/// Where a chroma at `ratio` of the boundary lands, `GAMUT_LIMIT` landing on
/// it exactly.
pub(crate) fn gamut_fold(ratio: f32) -> f32 {
    if ratio <= GAMUT_KNEE {
        return ratio;
    }
    let (t, l, p) = (GAMUT_KNEE, GAMUT_LIMIT, GAMUT_POWER);
    let scale = (l - t) / (((1.0 - t) / (l - t)).powf(-p) - 1.0).powf(1.0 / p);
    let x = (ratio - t) / scale;
    t + scale * x / (1.0 + x.powf(p)).powf(1.0 / p)
}

/// Oklab -> linear sRGB, at constant lightness and hue, with the chroma brought
/// inside what sRGB can show.
pub fn to_gamut(lightness: &Plane, a: &Plane, b: &Plane) -> Image {
    let mut out = Image::new(lightness.w, lightness.h);
    let fits = |r: f32, g: f32, bl: f32| r.min(g).min(bl) >= -0.001 && r.max(g).max(bl) <= 1.001;
    for i in 0..lightness.d.len() {
        let l = lightness.d[i].clamp(0.0, 1.0);
        let (aa, bb) = (a.d[i], b.d[i]);
        let (mut r, mut g, mut bl) = oklab_to_rgb_px(l, aa, bb);
        // Chroma this low is inside sRGB at any lightness a picture has, so the
        // search is spent only where it can matter.
        let chroma = aa.hypot(bb);
        let inside = fits(r, g, bl);
        if chroma > 0.03 || !inside {
            // How far the chroma may be scaled along this hue before leaving
            // the gamut, found by bisection: 1 is where the pixel is now.
            let mut hi = if inside { 1.0f32 / GAMUT_KNEE } else { 1.0 };
            if inside {
                let (tr, tg, tb) = oklab_to_rgb_px(l, aa * hi, bb * hi);
                if fits(tr, tg, tb) {
                    // Nowhere near the edge.
                    hi = f32::INFINITY;
                }
            }
            if hi.is_finite() {
                let mut lo = if inside { 1.0f32 } else { 0.0 };
                for _ in 0..12 {
                    let mid = 0.5 * (lo + hi);
                    let (tr, tg, tb) = oklab_to_rgb_px(l, aa * mid, bb * mid);
                    if fits(tr, tg, tb) {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                // Where this pixel sits against the boundary, 1 being on it.
                let ratio = 1.0 / lo.max(1e-6);
                if ratio > GAMUT_KNEE {
                    let target = gamut_fold(ratio);
                    // `lo` is the last scale known to fit, so the knee can
                    // never land a pixel outside it.
                    let scale = (target * lo).min(lo);
                    let (fr, fg, fb) = oklab_to_rgb_px(l, aa * scale, bb * scale);
                    r = fr;
                    g = fg;
                    bl = fb;
                }
            }
        }
        out.px_mut()[i] = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), bl.clamp(0.0, 1.0)];
    }
    out
}

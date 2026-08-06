//! The three things a curve cannot do.

use crate::ops::{self, Image};

/// The heaviest grain on offer, as a standard deviation in display-linear
/// units.
pub const GRAIN_MAX: f32 = 0.012;

/// The most the corners may be darkened. Vintage used 0.22.
pub const VIGNETTE_MAX: f32 = 0.35;

/// Where the corner falloff starts, as a fraction of the distance to the
/// corner. Every look in `styles.rs` used this same figure.
const VIGNETTE_START: f32 = 0.45;

/// How many grains fit across the long edge of the frame.
pub const GRAIN_ACROSS: f32 = 1300.0;

/// A standard normal value for a lattice point, the same every time it is asked
/// for.
fn lattice_normal(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    let mut next = || {
        // splitmix64
        h = h.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = h;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 40) as f32 / 16_777_216.0
    };
    let u1 = next().max(1e-7);
    let u2 = next();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
}

/// The grain field at a pixel, with unit variance at whatever scale it is
/// sampled, as the picture would look reduced to this size.
fn grain_at(x: usize, y: usize, per_pixel: f32) -> f32 {
    if per_pixel >= 1.0 {
        return lattice_normal(x as i64, y as i64) / per_pixel;
    }
    let (u, v) = ((x as f32 + 0.5) * per_pixel, (y as f32 + 0.5) * per_pixel);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let w = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let n = w[0] * lattice_normal(x0, y0) + w[1] * lattice_normal(x0 + 1, y0)
        + w[2] * lattice_normal(x0, y0 + 1) + w[3] * lattice_normal(x0 + 1, y0 + 1);
    n / (w.iter().map(|k| k * k).sum::<f32>()).sqrt()
}

/// Toward grey, by a panchromatic-with-a-yellow-filter mix.
pub fn monochrome(img: &mut Image, amount: f32) {
    let a = amount.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    for i in 0..img.w * img.h {
        let px = &img.d[i * 3..i * 3 + 3];
        let grey = 0.30 * px[0] + 0.60 * px[1] + 0.10 * px[2];
        for c in 0..3 {
            img.d[i * 3 + c] = img.d[i * 3 + c] * (1.0 - a) + grey * a;
        }
    }
}

/// Normalised distance from the centre, 1.0 at the corners.
fn radius_at(x: usize, y: usize, w: usize, h: usize) -> f32 {
    let cy = ((h as f32 - 1.0) / 2.0).max(1.0);
    let cx = ((w as f32 - 1.0) / 2.0).max(1.0);
    let ny = (y as f32 - (h as f32 - 1.0) / 2.0) / cy;
    let nx = (x as f32 - (w as f32 - 1.0) / 2.0) / cx;
    (nx * nx + ny * ny).sqrt() / std::f32::consts::SQRT_2
}

/// Darken toward the corners.
pub fn vignette(img: &mut Image, amount: f32) {
    let a = amount.clamp(0.0, 1.0) * VIGNETTE_MAX;
    if a <= 0.0 {
        return;
    }
    for y in 0..img.h {
        for x in 0..img.w {
            let falloff =
                1.0 - a * ops::smoothstep(VIGNETTE_START, 1.0, radius_at(x, y, img.w, img.h));
            let i = (y * img.w + x) * 3;
            for c in 0..3 {
                img.d[i + c] *= falloff;
            }
        }
    }
}

/// Luminance grain, strongest in the midtones as on real film.
pub fn grain(img: &mut Image, amount: f32) {
    let a = amount.clamp(0.0, 1.0) * GRAIN_MAX;
    if a <= 0.0 {
        return;
    }
    // Grains per pixel at this size.
    let per_pixel = GRAIN_ACROSS / img.w.max(img.h).max(1) as f32;
    for y in 0..img.h {
        for x in 0..img.w {
            let i = y * img.w + x;
            let px = &img.d[i * 3..i * 3 + 3];
            let luma = ops::luminance_px(px);
            let n = grain_at(x, y, per_pixel) * a * 4.0 * luma * (1.0 - luma);
            for c in 0..3 {
                img.d[i * 3 + c] = (img.d[i * 3 + c] + n).clamp(0.0, 1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, px: [f32; 3]) -> Image {
        let mut img = Image::new(w, h);
        for i in 0..w * h {
            for c in 0..3 {
                img.d[i * 3 + c] = px[c];
            }
        }
        img
    }

    /// Nothing asked for has to mean nothing done, to the last bit.
    #[test]
    fn zero_does_nothing_at_all() {
        let src = flat(8, 6, [0.3, 0.45, 0.6]);
        for effect in [monochrome as fn(&mut Image, f32), vignette, grain] {
            let mut img = src.clone();
            effect(&mut img, 0.0);
            assert_eq!(img.d, src.d);
        }
    }

    /// Full monochrome has to leave no colour behind, and half of it has to
    /// leave half: a blend that snaps is a checkbox wearing a slider's clothes.
    #[test]
    fn monochrome_drains_by_the_amount_asked() {
        let mut full = flat(4, 4, [0.8, 0.4, 0.2]);
        monochrome(&mut full, 1.0);
        let grey = 0.30 * 0.8 + 0.60 * 0.4 + 0.10 * 0.2;
        for c in 0..3 {
            assert!((full.d[c] - grey).abs() < 1e-6, "channel {c} is {} not {grey}", full.d[c]);
        }
        let mut half = flat(4, 4, [0.8, 0.4, 0.2]);
        monochrome(&mut half, 0.5);
        for (c, was) in [0.8f32, 0.4, 0.2].iter().enumerate() {
            let want = 0.5 * was + 0.5 * grey;
            assert!((half.d[c] - want).abs() < 1e-6, "half drained to {}", half.d[c]);
        }
    }

    /// The centre is what a vignette must not touch, and the corners are what
    /// it is for.
    #[test]
    fn a_vignette_holds_the_centre_and_scales_with_the_frame() {
        for (w, h) in [(101usize, 101usize), (1001, 1001)] {
            let mut img = flat(w, h, [0.5, 0.5, 0.5]);
            vignette(&mut img, 1.0);
            let centre = (h / 2) * w + w / 2;
            assert!((img.d[centre * 3] - 0.5).abs() < 1e-6,
                    "{w}x{h} moved the centre to {}", img.d[centre * 3]);
            let corner = img.d[0];
            let want = 0.5 * (1.0 - VIGNETTE_MAX);
            assert!((corner - want).abs() < 1e-4,
                    "{w}x{h} corner is {corner} rather than {want}");
        }
    }

    /// Grain has to be the same grain twice, or an export does not match the
    /// preview it was approved from.
    #[test]
    fn grain_survives_being_reduced() {
        let spread = |img: &Image| -> f32 {
            let v: Vec<f32> = img.d.chunks_exact(3).map(|p| p[1]).collect();
            ops::std_dev(&v)
        };
        let mut small = flat(650, 400, [0.5, 0.5, 0.5]);
        let mut large = flat(2600, 1600, [0.5, 0.5, 0.5]);
        grain(&mut small, 1.0);
        grain(&mut large, 1.0);
        let reduced = ops::resize_rgb(&large, 650, 400);
        let (s, r) = (spread(&small), spread(&reduced));
        assert!((r / s - 1.0).abs() < 0.2, "grain changed with size: {s} at the small size, {r} reduced");
    }

    #[test]
    fn grain_repeats_exactly() {
        let src = flat(32, 32, [0.5, 0.5, 0.5]);
        let mut a = src.clone();
        let mut b = src.clone();
        grain(&mut a, 0.6);
        grain(&mut b, 0.6);
        assert_eq!(a.d, b.d, "the same settings grained differently");
        assert_ne!(a.d, src.d, "grain did nothing");
    }

    /// Where there is no silver left to be uneven there is no grain.
    #[test]
    fn grain_leaves_the_ends_alone() {
        for level in [0.0f32, 1.0] {
            let src = flat(32, 32, [level; 3]);
            let mut img = src.clone();
            grain(&mut img, 1.0);
            assert_eq!(img.d, src.d, "grain reached a pixel at {level}");
        }
    }
}

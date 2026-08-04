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

/// Deterministic noise: the same photograph grains the same way twice.
struct Rng(u64);

impl Rng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let xorshifted = (((self.0 >> 18) ^ self.0) >> 27) as u32;
        let rot = (self.0 >> 59) as u32;
        xorshifted.rotate_right(rot)
    }
    fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    fn normal(&mut self) -> f32 {
        let u1 = self.unit().max(1e-7);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
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
    // A fixed seed, because the same settings on the same photograph have to
    // give the same file.
    let mut rng = Rng(7);
    for i in 0..img.w * img.h {
        let px = &img.d[i * 3..i * 3 + 3];
        let y = ops::luminance_px(px);
        let n = rng.normal() * a * 4.0 * y * (1.0 - y);
        for c in 0..3 {
            img.d[i * 3 + c] = (img.d[i * 3 + c] + n).clamp(0.0, 1.0);
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

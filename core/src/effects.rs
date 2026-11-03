//! The three things a curve cannot do.

use crate::grade::Window;
use kit::Image;

/// The heaviest grain on offer, as a standard deviation in display-linear
/// units.
pub const GRAIN_MAX: f32 = 0.012;

/// The most the corners may be darkened.
pub const VIGNETTE_MAX: f32 = 0.35;

/// Where the corner falloff starts, as a fraction of the distance to the
/// corner.
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

/// Toward grey, by a panchromatic-with-a-yellow-filter mix.
pub fn monochrome(img: &mut Image, amount: f32) {
    let a = amount.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    for px in img.px_mut() {
        let grey = 0.30 * px[0] + 0.60 * px[1] + 0.10 * px[2];
        for v in px.iter_mut() {
            *v = *v * (1.0 - a) + grey * a;
        }
    }
}

/// Darken toward the corners of the frame `window` places this image in.
pub fn vignette_in(img: &mut Image, amount: f32, window: Window) {
    let a = amount.clamp(0.0, 1.0) * VIGNETTE_MAX;
    if a <= 0.0 {
        return;
    }
    let (w, h) = (window.w as f32, window.h as f32);
    let cx = ((w - 1.0) / 2.0).max(1.0);
    let cy = ((h - 1.0) / 2.0).max(1.0);
    let width = img.w;
    for (y, row) in img.px_mut().chunks_exact_mut(width).enumerate() {
        let ny = ((y + window.y) as f32 - (h - 1.0) / 2.0) / cy;
        for (x, px) in row.iter_mut().enumerate() {
            let nx = ((x + window.x) as f32 - (w - 1.0) / 2.0) / cx;
            // Normalised distance from the centre, 1.0 at the corners.
            let r = (nx * nx + ny * ny).sqrt() / std::f32::consts::SQRT_2;
            let falloff = 1.0 - a * kit::smoothstep(VIGNETTE_START, 1.0, r);
            for v in px.iter_mut() {
                *v *= falloff;
            }
        }
    }
}

/// Luminance grain, strongest in the midtones as on real film. On a piece of
/// a frame it lays down the grains the whole frame has there, and it keeps
/// unit variance at any size, as the picture would look reduced to it.
pub fn grain_in(img: &mut Image, amount: f32, window: Window) {
    let a = amount.clamp(0.0, 1.0) * GRAIN_MAX;
    if a <= 0.0 {
        return;
    }
    let per_pixel = GRAIN_ACROSS / window.long_edge().max(1) as f32;
    let w = img.w;
    let mut lattice = LatticeRows::default();
    for (y, row) in img.px_mut().chunks_mut(w).enumerate() {
        let y = y + window.y;
        let v = (y as f32 + 0.5) * per_pixel;
        let (y0, fy) = (v.floor(), v - v.floor());
        if per_pixel < 1.0 {
            let first = ((window.x as f32 + 0.5) * per_pixel).floor() as i64;
            let last = (((window.x + w) as f32 + 0.5) * per_pixel).floor() as i64 + 1;
            lattice.fill(y0 as i64, first, last);
        }
        for (x, px) in row.iter_mut().enumerate() {
            let x = x + window.x;
            let n = if per_pixel >= 1.0 {
                lattice_normal(x as i64, y as i64) / per_pixel
            } else {
                let u = (x as f32 + 0.5) * per_pixel;
                let x0 = u.floor();
                let fx = u - x0;
                let k = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
                let (top, bottom) = lattice.at(x0 as i64);
                let n = k[0] * top[0] + k[1] * top[1] + k[2] * bottom[0] + k[3] * bottom[1];
                n / (k.iter().map(|k| k * k).sum::<f32>()).sqrt()
            };
            let luma = kit::luminance_px(px);
            let n = n * a * 4.0 * luma * (1.0 - luma);
            for v in px.iter_mut() {
                *v = (*v + n).clamp(0.0, 1.0);
            }
        }
    }
}

/// Two rows of lattice values, kept while the rows of pixels being grained
/// fall between them: below full size, several pixels share every point.
#[derive(Default)]
struct LatticeRows {
    y0: Option<i64>,
    first: i64,
    top: Vec<f32>,
    bottom: Vec<f32>,
}

impl LatticeRows {
    fn fill(&mut self, y0: i64, first: i64, last: i64) {
        if self.y0 == Some(y0) {
            return;
        }
        let row = |y: i64| (first..=last).map(|x| lattice_normal(x, y)).collect::<Vec<_>>();
        if self.y0 == Some(y0 - 1) && self.first == first {
            self.top = std::mem::take(&mut self.bottom);
        } else {
            self.top = row(y0);
        }
        self.bottom = row(y0 + 1);
        self.y0 = Some(y0);
        self.first = first;
    }

    /// The points at `x0` and `x0 + 1` on the top row and on the bottom one.
    fn at(&self, x0: i64) -> ([f32; 2], [f32; 2]) {
        let i = (x0 - self.first) as usize;
        ([self.top[i], self.top[i + 1]], [self.bottom[i], self.bottom[i + 1]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, px: [f32; 3]) -> Image {
        Image::filled(w, h, px)
    }

    fn vignette(img: &mut Image, amount: f32) {
        vignette_in(img, amount, Window::whole(img));
    }

    fn grain(img: &mut Image, amount: f32) {
        grain_in(img, amount, Window::whole(img));
    }

    /// Zero must leave every bit as it was.
    #[test]
    fn zero_does_nothing_at_all() {
        let src = flat(8, 6, [0.3, 0.45, 0.6]);
        for effect in [monochrome as fn(&mut Image, f32), vignette as fn(&mut Image, f32), grain] {
            let mut img = src.clone();
            effect(&mut img, 0.0);
            assert_eq!(img.d, src.d);
        }
    }

    /// Full monochrome leaves no colour behind, and half of it leaves half.
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

    /// The export has to carry the grain the preview showed.
    #[test]
    fn grain_survives_being_reduced() {
        let spread = |img: &Image| -> f32 {
            let v: Vec<f32> = img.d.chunks_exact(3).map(|p| p[1]).collect();
            kit::std_dev(&v)
        };
        let mut small = flat(650, 400, [0.5, 0.5, 0.5]);
        let mut large = flat(2600, 1600, [0.5, 0.5, 0.5]);
        grain(&mut small, 1.0);
        grain(&mut large, 1.0);
        let reduced = kit::resize_rgb(&large, 650, 400);
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

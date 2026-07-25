//! Looks applied on top of a finished, corrected image.

use crate::grade::{s_curve, to_gamut};
use crate::ops::{self, Image, Plane};

pub struct Style {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The look itself, or nothing at all for the original.
    pub look: Option<fn(&Image) -> Image>,
}

pub const STYLES: [Style; 10] = [
    Style { id: "original", label: "Original", description: "Your corrections, nothing added",
            look: None },
    Style { id: "punch", label: "Punch", description: "Extra local contrast and colour",
            look: Some(punch) },
    Style { id: "cinematic", label: "Cinematic", description: "Teal shadows, warm highlights",
            look: Some(cinematic) },
    Style { id: "film", label: "Film", description: "Warm portrait stock, soft blacks",
            look: Some(film) },
    Style { id: "bw", label: "Black & white", description: "Yellow-filter panchromatic, grain",
            look: Some(bw) },
    Style { id: "bleach", label: "Bleach bypass", description: "Desaturated, hard contrast",
            look: Some(bleach) },
    Style { id: "cold", label: "Cold", description: "Blue-green cast, crisp",
            look: Some(cold) },
    Style { id: "vintage", label: "Vintage", description: "Faded, warm, vignetted",
            look: Some(vintage) },
    Style { id: "dream", label: "Dream", description: "Orton glow, soft highlights",
            look: Some(dream) },
    Style { id: "chroma", label: "Chromatic", description: "Artistic colour fringing and bloom",
            look: Some(chroma) },
];

/// The look of that name, or nothing if there is no such look.
pub fn find(id: &str) -> Option<&'static Style> {
    STYLES.iter().find(|s| s.id == id)
}

// -------------------------------------------------------------------------
// helpers
// -------------------------------------------------------------------------

/// Normalised distance from centre, 1.0 at the corners.
fn radius_at(x: usize, y: usize, w: usize, h: usize) -> f32 {
    let cy = ((h as f32 - 1.0) / 2.0).max(1.0);
    let cx = ((w as f32 - 1.0) / 2.0).max(1.0);
    let ny = (y as f32 - (h as f32 - 1.0) / 2.0) / cy;
    let nx = (x as f32 - (w as f32 - 1.0) / 2.0) / cx;
    (nx * nx + ny * ny).sqrt() / std::f32::consts::SQRT_2
}

fn vignette(img: &mut Image, amount: f32, start: f32) {
    for y in 0..img.h {
        for x in 0..img.w {
            let falloff = 1.0 - amount * ops::smoothstep(start, 1.0, radius_at(x, y, img.w, img.h));
            let i = (y * img.w + x) * 3;
            for c in 0..3 {
                img.d[i + c] *= falloff;
            }
        }
    }
}

/// Deterministic noise: same input, same file.  (numpy's Generator is not
/// reproducible outside numpy, so this is our own PCG plus Box-Muller.)
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

/// Luminance grain, strongest in the midtones as on real film.
fn grain(img: &mut Image, amount: f32, seed: u64) {
    let mut rng = Rng(seed);
    for i in 0..img.w * img.h {
        let px = &img.d[i * 3..i * 3 + 3];
        let y = ops::luminance_px(px);
        let weight = 4.0 * y * (1.0 - y);
        let n = rng.normal() * amount * weight;
        for c in 0..3 {
            img.d[i * 3 + c] = (img.d[i * 3 + c] + n).clamp(0.0, 1.0);
        }
    }
}

fn split_tone(img: &Image, shadow: (f32, f32), highlight: (f32, f32), strength: f32) -> Image {
    let n = img.w * img.h;
    let mut l = Plane::new(img.w, img.h);
    let mut a = Plane::new(img.w, img.h);
    let mut b = Plane::new(img.w, img.h);
    for i in 0..n {
        let (ll, aa, bb) = ops::rgb_to_oklab_px(img.d[i * 3], img.d[i * 3 + 1], img.d[i * 3 + 2]);
        let w_hi = ops::smoothstep(0.35, 0.85, ll);
        let w_lo = 1.0 - ops::smoothstep(0.10, 0.60, ll);
        l.d[i] = ll;
        a.d[i] = aa + strength * (shadow.0 * w_lo + highlight.0 * w_hi);
        b.d[i] = bb + strength * (shadow.1 * w_lo + highlight.1 * w_hi);
    }
    to_gamut(&l, &a, &b)
}

fn contrast(img: &Image, amount: f32, pivot: f32) -> Image {
    let n = img.w * img.h;
    let mut l = Plane::new(img.w, img.h);
    let mut a = Plane::new(img.w, img.h);
    let mut b = Plane::new(img.w, img.h);
    for i in 0..n {
        let (ll, aa, bb) = ops::rgb_to_oklab_px(img.d[i * 3], img.d[i * 3 + 1], img.d[i * 3 + 2]);
        l.d[i] = s_curve(ll, amount, pivot);
        a.d[i] = aa;
        b.d[i] = bb;
    }
    to_gamut(&l, &a, &b)
}

fn saturate(img: &Image, factor: f32) -> Image {
    let n = img.w * img.h;
    let mut l = Plane::new(img.w, img.h);
    let mut a = Plane::new(img.w, img.h);
    let mut b = Plane::new(img.w, img.h);
    for i in 0..n {
        let (ll, aa, bb) = ops::rgb_to_oklab_px(img.d[i * 3], img.d[i * 3 + 1], img.d[i * 3 + 2]);
        l.d[i] = ll;
        a.d[i] = aa * factor;
        b.d[i] = bb * factor;
    }
    to_gamut(&l, &a, &b)
}

/// Matte/faded blacks: raise the floor without touching the white point.
fn lift(img: &mut Image, amount: f32) {
    for v in img.d.iter_mut() {
        *v = *v * (1.0 - amount) + amount;
    }
}

/// Scale a channel about the frame centre, the mechanism behind CA.
fn radial_scale(p: &Plane, k: f32) -> Plane {
    let mut out = Plane::new(p.w, p.h);
    let cy = (p.h as f32 - 1.0) / 2.0;
    let cx = (p.w as f32 - 1.0) / 2.0;
    for y in 0..p.h {
        for x in 0..p.w {
            let sx = cx + (x as f32 - cx) / (1.0 + k);
            let sy = cy + (y as f32 - cy) / (1.0 + k);
            out.d[y * p.w + x] = ops::sample_bilinear(p, sx, sy);
        }
    }
    out
}

/// Screen a blurred copy of the highlights back over the frame.
fn bloom(img: &Image, threshold: f32, amount: f32, sigma: f32) -> Image {
    let y = ops::luminance(img);
    let mut masked = [Plane::new(img.w, img.h), Plane::new(img.w, img.h), Plane::new(img.w, img.h)];
    for i in 0..img.w * img.h {
        let m = ops::smoothstep(threshold, 1.0, y.d[i]);
        for c in 0..3 {
            masked[c].d[i] = img.d[i * 3 + c] * m;
        }
    }
    let glow: Vec<Plane> = masked.iter().map(|p| ops::gaussian_blur(p, sigma)).collect();
    let mut out = img.clone();
    for i in 0..img.w * img.h {
        for c in 0..3 {
            let g = (glow[c].d[i] * amount).clamp(0.0, 1.0);
            out.d[i * 3 + c] = 1.0 - (1.0 - img.d[i * 3 + c]) * (1.0 - g);
        }
    }
    out
}

/// Blur radii in *relative* terms so a look survives any output size.
fn sigma_for(img: &Image, fraction: f32) -> f32 {
    (fraction * img.w.max(img.h) as f32).max(1.0)
}

// -------------------------------------------------------------------------
// the looks
// -------------------------------------------------------------------------

pub fn apply(img: &Image, style: &Style) -> Image {
    let Some(look) = style.look else { return img.clone() };
    let mut out = look(img);
    for v in out.d.iter_mut() {
        *v = v.clamp(0.0, 1.0);
    }
    out
}

fn bw(img: &Image) -> Image {
    // Panchromatic-with-a-yellow-filter weighting: skies darken, skin holds.
    let mut mono = Image::new(img.w, img.h);
    for i in 0..img.w * img.h {
        let y = 0.30 * img.d[i * 3] + 0.60 * img.d[i * 3 + 1] + 0.10 * img.d[i * 3 + 2];
        for c in 0..3 {
            mono.d[i * 3 + c] = y;
        }
    }
    let mut out = contrast(&mono, 0.35, 0.45);
    grain(&mut out, 0.006, 7);
    out
}

fn cinematic(img: &Image) -> Image {
    let out = split_tone(img, (-0.035, -0.030), (0.022, 0.030), 1.0);
    let mut out = contrast(&out, 0.30, 0.42);
    vignette(&mut out, 0.18, 0.45);
    out
}

fn film(img: &Image) -> Image {
    let out = split_tone(img, (-0.008, 0.014), (0.014, 0.020), 0.9);
    let mut out = saturate(&out, 0.94);
    lift(&mut out, 0.025);
    grain(&mut out, 0.005, 7);
    out
}

fn bleach(img: &Image) -> Image {
    let mut overlaid = Image::new(img.w, img.h);
    for i in 0..img.w * img.h {
        let y = ops::luminance_px(&img.d[i * 3..i * 3 + 3]);
        for c in 0..3 {
            let v = img.d[i * 3 + c];
            let overlay = if y < 0.5 { 2.0 * v * y } else { 1.0 - 2.0 * (1.0 - v) * (1.0 - y) };
            overlaid.d[i * 3 + c] = (v * 0.35 + overlay * 0.65).clamp(0.0, 1.0);
        }
    }
    let out = saturate(&overlaid, 0.55);
    contrast(&out, 0.30, 0.42)
}

fn chroma(img: &Image) -> Image {
    let k = 0.0035;
    let r = radial_scale(&img.plane(0), k);
    let g = img.plane(1);
    let b = radial_scale(&img.plane(2), -k);
    let shifted = Image::from_planes(&r, &g, &b);
    let s = sigma_for(&shifted, 0.012);
    let mut out = bloom(&shifted, 0.75, 0.35, s);
    vignette(&mut out, 0.12, 0.45);
    out
}

fn dream(img: &Image) -> Image {
    let s = sigma_for(img, 0.018);
    let glow: Vec<Plane> = (0..3).map(|c| ops::gaussian_blur(&img.plane(c), s)).collect();
    let mut mixed = Image::new(img.w, img.h);
    for i in 0..img.w * img.h {
        for c in 0..3 {
            let v = img.d[i * 3 + c];
            let soft = 1.0 - (1.0 - v) * (1.0 - glow[c].d[i] * 0.55); // Orton
            mixed.d[i * 3 + c] = (v * 0.55 + soft * 0.45).clamp(0.0, 1.0);
        }
    }
    saturate(&mixed, 1.12)
}

fn vintage(img: &Image) -> Image {
    let out = split_tone(img, (0.010, 0.012), (0.018, 0.026), 0.8);
    let mut out = saturate(&out, 0.78);
    lift(&mut out, 0.07);
    vignette(&mut out, 0.22, 0.45);
    grain(&mut out, 0.008, 7);
    out
}

fn cold(img: &Image) -> Image {
    let out = split_tone(img, (-0.030, -0.038), (-0.010, -0.014), 1.0);
    let out = saturate(&out, 1.05);
    contrast(&out, 0.25, 0.42)
}

fn punch(img: &Image) -> Image {
    // Local contrast rather than global: what "clarity" actually does.
    let y = ops::luminance(img);
    let radius = ((img.w.max(img.h) / 60) as usize).max(8);
    let base = ops::guided_filter(&y, &y, radius, 0.004, 4);
    let mut scaled = Image::new(img.w, img.h);
    for i in 0..img.w * img.h {
        let scale = ((y.d[i] - base.d[i]) * 0.55 + 1.0).clamp(0.6, 1.6);
        for c in 0..3 {
            scaled.d[i * 3 + c] = (img.d[i * 3 + c] * scale).clamp(0.0, 1.0);
        }
    }
    saturate(&scaled, 1.12)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name that is not in the table must not resolve to anything.
    #[test]
    fn a_name_that_is_not_a_look_does_not_become_one() {
        assert!(find("warm").is_none(), "made-up names must not resolve");
        assert!(find("BW").is_none(), "and the match is exact");
        assert!(find("").is_none());
        assert!(find("bw").is_some_and(|s| s.look.is_some()));
    }

    /// The original is a choice in the list and the absence of a look at the
    /// same time, which is the one case `apply` passes straight through.
    #[test]
    fn the_original_is_the_absence_of_a_look() {
        let original = find("original").expect("the original is always offered");
        assert!(original.look.is_none());
        let img = Image::new(4, 4);
        assert_eq!(apply(&img, original).d, img.d);
    }

    /// Everything else in the table has to actually do something, or it would
    /// be an unstyled export wearing a name.
    #[test]
    fn every_other_look_has_one() {
        for style in STYLES.iter().filter(|s| s.id != "original") {
            assert!(style.look.is_some(), "{} has no look attached", style.id);
        }
        for (i, style) in STYLES.iter().enumerate() {
            assert!(STYLES.iter().skip(i + 1).all(|other| other.id != style.id),
                    "{} is in the table twice", style.id);
        }
    }
}

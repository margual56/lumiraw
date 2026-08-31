//! Eight hue bands, each with a hue, a saturation and a lightness.

use kit::to_gamut;
use kit::Plane;

/// The eight bands, by the name each is known by.
pub const BANDS: [&str; 8] =
    ["red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta"];

/// The sRGB colour each band is centred on.
const ANCHORS: [[f32; 3]; 8] = [
    [1.0, 0.0, 0.0],
    [1.0, 0.5, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 1.0, 1.0],
    [0.0, 0.0, 1.0],
    [0.5, 0.0, 1.0],
    [1.0, 0.0, 1.0],
];

/// The most a band may turn a hue, in radians.
const MAX_HUE: f32 = 0.44;

/// The most a band may scale chroma, either way.
const MAX_SAT: f32 = 1.0;

/// The most a band may scale lightness.
const MAX_LUM: f32 = 0.45;

/// Below this chroma a pixel is grey and has no hue worth acting on.
const GREY: f32 = 0.02;
const COLOURED: f32 = 0.055;

/// What each band has been asked to do: hue, saturation, lightness, each
/// -1..1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mixer {
    pub hue: [f32; 8],
    pub sat: [f32; 8],
    pub lum: [f32; 8],
}

impl Mixer {
    /// Whether this would change anything.
    pub fn is_identity(&self) -> bool {
        self.hue.iter().chain(&self.sat).chain(&self.lum).all(|v| v.abs() < 1e-4)
    }

    /// The mixer a settings object asks for.
    pub fn from_json(v: Option<&serde_json::Value>) -> Mixer {
        let mut m = Mixer::default();
        let Some(v) = v else { return m };
        for (i, name) in BANDS.iter().enumerate() {
            let Some(band) = v.get(name) else { continue };
            let read = |k: &str| band.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            m.hue[i] = read("h").clamp(-1.0, 1.0);
            m.sat[i] = read("s").clamp(-1.0, 1.0);
            m.lum[i] = read("l").clamp(-1.0, 1.0);
        }
        m
    }
}

/// The Oklab hue angle of each band's anchor, sorted around the circle.
fn centres() -> [(f32, usize); 8] {
    let mut out = [(0.0f32, 0usize); 8];
    for (i, rgb) in ANCHORS.iter().enumerate() {
        let lin = rgb.map(kit::srgb_decode_scalar);
        let (_, a, b) = kit::rgb_to_oklab_px(lin[0], lin[1], lin[2]);
        out[i] = (b.atan2(a), i);
    }
    out.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    out
}

/// Apply the mixer to a display-linear image.
pub fn apply(img: &kit::Image, m: &Mixer) -> kit::Image {
    if m.is_identity() {
        return img.clone();
    }
    let ring = centres();
    let tau = std::f32::consts::TAU;
    let mut l = Plane::new(img.w, img.h);
    let mut pa = Plane::new(img.w, img.h);
    let mut pb = Plane::new(img.w, img.h);

    for (i, px) in img.px().iter().enumerate() {
        let (ll, a, b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
        let chroma = (a * a + b * b).sqrt();
        let reach = kit::smoothstep(GREY, COLOURED, chroma);
        if reach <= 0.0 {
            l.d[i] = ll;
            pa.d[i] = a;
            pb.d[i] = b;
            continue;
        }
        let hue = b.atan2(a);

        // The two centres this hue sits between, and how far along it is.
        let mut lo = 7usize;
        for (k, (angle, _)) in ring.iter().enumerate() {
            if *angle > hue {
                lo = (k + 7) % 8;
                break;
            }
        }
        let hi = (lo + 1) % 8;
        let span = (ring[hi].0 - ring[lo].0).rem_euclid(tau);
        let along = (hue - ring[lo].0).rem_euclid(tau) / span.max(1e-6);
        let w_hi = kit::smoothstep(0.0, 1.0, along.clamp(0.0, 1.0));
        let (band_lo, band_hi) = (ring[lo].1, ring[hi].1);

        let mix = |v: &[f32; 8]| (v[band_lo] * (1.0 - w_hi) + v[band_hi] * w_hi) * reach;
        let turn = mix(&m.hue) * MAX_HUE;
        let sat = 1.0 + mix(&m.sat) * MAX_SAT;
        let lum = 1.0 + mix(&m.lum) * MAX_LUM;

        let (sin, cos) = (hue + turn).sin_cos();
        let radius = chroma * sat.max(0.0);
        l.d[i] = (ll * lum).clamp(0.0, 1.0);
        pa.d[i] = cos * radius;
        pb.d[i] = sin * radius;
    }
    to_gamut(&l, &pa, &pb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kit::Image;

    fn flat(px: [f32; 3]) -> Image {
        Image::filled(4, 4, px)
    }

    fn srgb(px: [f32; 3]) -> [f32; 3] {
        px.map(kit::srgb_decode_scalar)
    }

    fn chroma_of(img: &Image) -> f32 {
        let (_, a, b) = kit::rgb_to_oklab_px(img.d[0], img.d[1], img.d[2]);
        (a * a + b * b).sqrt()
    }

    /// Nothing asked for has to be nothing done, to the bit: this runs on every
    /// render and a full Oklab round trip is not free of error.
    #[test]
    fn an_untouched_mixer_changes_nothing() {
        let src = flat(srgb([0.6, 0.3, 0.2]));
        let out = apply(&src, &Mixer::default());
        assert_eq!(out.d, src.d);
    }

    /// The eight centres have to be eight distinct hues going round once, or
    /// the band a pixel lands in is a coin toss.
    #[test]
    fn the_eight_centres_are_distinct_and_ordered() {
        let ring = centres();
        for pair in ring.windows(2) {
            assert!(pair[1].0 > pair[0].0, "centres out of order: {ring:?}");
            assert!(pair[1].0 - pair[0].0 > 0.2, "two bands sit on top of each other: {ring:?}");
        }
        let mut seen: Vec<usize> = ring.iter().map(|(_, i)| *i).collect();
        seen.sort();
        assert_eq!(seen, (0..8).collect::<Vec<_>>(), "a band is missing from the ring");
    }

    /// Saturation on the band a colour belongs to has to reach it, and the one
    /// on the far side of the wheel has to leave it alone.
    #[test]
    fn a_band_reaches_its_own_colour_and_not_the_opposite_one() {
        let blue = flat(srgb([0.15, 0.35, 0.85]));
        let base = chroma_of(&blue);

        let mut m = Mixer::default();
        m.sat[BANDS.iter().position(|b| *b == "blue").unwrap()] = 1.0;
        let lifted = chroma_of(&apply(&blue, &m));
        assert!(lifted > base * 1.3, "blue did not reach blue: {base} to {lifted}");

        let mut m = Mixer::default();
        m.sat[BANDS.iter().position(|b| *b == "orange").unwrap()] = -1.0;
        let untouched = chroma_of(&apply(&blue, &m));
        assert!((untouched - base).abs() < base * 0.05,
                "orange reached a blue pixel: {base} to {untouched}");
    }

    /// Grey has no hue, only arithmetic noise where one would be. Tinting it
    /// is how a mixer turns shadow noise into coloured blotches.
    #[test]
    fn grey_is_left_alone() {
        for level in [0.1f32, 0.45, 0.8] {
            let src = flat([level; 3]);
            let mut m = Mixer::default();
            // Every band pushed as far as it goes, in every direction.
            for i in 0..8 {
                m.hue[i] = 1.0;
                m.sat[i] = 1.0;
                m.lum[i] = if i % 2 == 0 { 1.0 } else { -1.0 };
            }
            let out = apply(&src, &m);
            for c in 0..3 {
                assert!((out.d[c] - level).abs() < 2e-3,
                        "grey at {level} moved to {} in channel {c}", out.d[c]);
            }
        }
    }

    /// Moving all eight the same way has to be exactly a global move, which is
    /// the property that proves the weights sum to one everywhere.
    #[test]
    fn all_eight_together_is_a_global_move() {
        let mut every = Mixer::default();
        for i in 0..8 {
            every.sat[i] = 0.4;
        }
        for px in [[0.70, 0.52, 0.52], [0.70, 0.62, 0.50],
                   [0.50, 0.68, 0.55], [0.52, 0.58, 0.72]] {
            let src = flat(srgb(px));
            let out = apply(&src, &every);
            let ratio = chroma_of(&out) / chroma_of(&src);
            assert!((ratio - 1.4).abs() < 0.06,
                    "{px:?} scaled by {ratio} rather than by 1.4");
        }
    }

    /// A hue turn has to turn the hue and leave the chroma roughly where it
    /// was, or the control is two controls wearing one name.
    #[test]
    fn a_hue_turn_turns_the_hue() {
        let green = flat(srgb([0.45, 0.66, 0.5]));
        let (_, a0, b0) = kit::rgb_to_oklab_px(green.d[0], green.d[1], green.d[2]);
        let mut m = Mixer::default();
        m.hue[BANDS.iter().position(|b| *b == "green").unwrap()] = 1.0;
        let out = apply(&green, &m);
        let (_, a1, b1) = kit::rgb_to_oklab_px(out.d[0], out.d[1], out.d[2]);
        let turned = (b1.atan2(a1) - b0.atan2(a0)).abs();
        assert!(turned > 0.15, "the hue barely moved: {turned} radians");
        let (c0, c1) = ((a0 * a0 + b0 * b0).sqrt(), (a1 * a1 + b1 * b1).sqrt());
        assert!((c1 / c0 - 1.0).abs() < 0.25, "the turn changed the chroma by {}", c1 / c0);
    }
}

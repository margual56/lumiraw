//! Haze taken out, or put in: the dark-channel prior (He, Sun and Tang, 2009).
//!
//! Outdoors, almost every small patch of a clear picture has some channel
//! close to black: a shadow, a dark object, a saturated colour. Haze lifts
//! those toward the colour of the air, more the further away things are. So
//! the darkest channel in each patch, measured against the air's colour, says
//! how much haze lies over it (`transmission`), and the haze can be
//! subtracted: `scene = (seen − air) / t + air`.
//!
//! On the scene-linear frame, after exposure and before the tone map, where
//! light still adds up the way the model says it does.

use kit::{Image, Plane};

/// The least transmission ever assumed, so the far distance is not pulled
/// into noise.
const FLOOR: f32 = 0.2;

/// The minimum of a run of `inner` values `at(i)` over a window of `r` either
/// side, at a constant cost a value whatever `r` is (van Herk, Gil and Werman):
/// running minima forward and backward within blocks of `2r + 1`.
fn min_run(inner: usize, r: usize, at: impl Fn(usize) -> f32, mut put: impl FnMut(usize, f32)) {
    let k = 2 * r + 1;
    let n = inner + 2 * r;
    // Padded with the edge values, so the window shrinks at the edges.
    let v = |i: usize| at(i.saturating_sub(r).min(inner - 1));
    let mut fwd = vec![0f32; n];
    let mut bwd = vec![0f32; n];
    for i in 0..n {
        fwd[i] = if i % k == 0 { v(i) } else { fwd[i - 1].min(v(i)) };
    }
    for i in (0..n).rev() {
        bwd[i] = if i % k == k - 1 || i == n - 1 { v(i) } else { bwd[i + 1].min(v(i)) };
    }
    for i in 0..inner {
        // The window in padded terms is i .. i + 2r.
        put(i, bwd[i].min(fwd[i + 2 * r]));
    }
}

/// A plane's minimum over a square of `r` pixels either side.
fn min_filter(p: &Plane, r: usize) -> Plane {
    let mut rows = Plane::new(p.w, p.h);
    for y in 0..p.h {
        min_run(p.w, r, |x| p.d[y * p.w + x], |x, m| rows.d[y * p.w + x] = m);
    }
    let mut out = Plane::new(p.w, p.h);
    for x in 0..p.w {
        min_run(p.h, r, |y| rows.d[y * p.w + x], |y, m| out.d[y * p.w + x] = m);
    }
    out
}

/// The darkest channel of each pixel over `air`, then over a patch.
fn dark_channel(img: &Image, air: [f32; 3], r: usize) -> Plane {
    let mut p = Plane::new(img.w, img.h);
    for (v, px) in p.d.iter_mut().zip(img.px()) {
        *v = (0..3).map(|c| px[c] / air[c].max(1e-6)).fold(f32::MAX, f32::min);
    }
    min_filter(&p, r)
}

/// The colour of the air: the mean of the haziest thousandth of the picture
/// (brightest in its dark channel), which is where it shows least diluted.
pub fn air_light(thumb: &Image) -> [f32; 3] {
    let r = (thumb.w.max(thumb.h) / 60).max(2);
    let dark = dark_channel(thumb, [1.0; 3], r);
    let mut order: Vec<usize> = (0..dark.d.len()).collect();
    order.sort_by(|a, b| dark.d[*b].partial_cmp(&dark.d[*a]).unwrap());
    let take = (order.len() / 1000).max(1);
    let mut sum = [0f32; 3];
    for &i in &order[..take] {
        for c in 0..3 {
            sum[c] += thumb.px()[i][c];
        }
    }
    sum.map(|s| (s / take as f32).max(1e-4))
}

/// Haze out (`amount` above zero) or in (below), up to all of it at ±1, for
/// air of the colour `air`. `long_edge` is the whole frame's, so a piece of it
/// measures haze over the same size of patch as the whole does.
pub fn apply(img: &mut Image, air: [f32; 3], amount: f32, long_edge: usize) {
    if amount.abs() < 1e-3 {
        return;
    }
    // Patches a sixtieth of the frame across, at full size so a piece of the
    // frame measures exactly what the whole does there; then fitted to the
    // picture's own edges.
    let r = (long_edge / 120).max(1);
    let dark = dark_channel(img, air, r);
    let mut t = dark;
    for t in t.d.iter_mut() {
        // He et al. leave a little haze in (0.95), so distance still reads.
        *t = 1.0 - 0.95 * t.clamp(0.0, 1.0);
    }
    let mut guide = kit::luminance(img);
    guide.d.iter_mut().for_each(|v| *v = v.max(0.0).sqrt());
    let radius = (long_edge / 120).max(4);
    // A small epsilon: the transmission has to step where an edge does, or a
    // building against the sky wears a halo of haze.
    let t = kit::guided_filter(&guide, &t, radius, 1e-4, (radius / 4).max(1));
    for (px, t) in img.px_mut().iter_mut().zip(&t.d) {
        let t = t.clamp(FLOOR, 1.0);
        for c in 0..3 {
            let v = px[c];
            px[c] = if amount > 0.0 {
                // Part of the way to the clear scene.
                let clear = (v - air[c]) / t + air[c];
                v + (clear - v) * amount
            } else {
                // More of the air over what is already far away.
                let veil = -amount * 0.6 * (1.0 - t);
                v * (1.0 - veil) + air[c] * veil
            }
            .max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A textured scene, and the same scene seen through haze that thickens
    /// toward the top.
    fn scene() -> (Image, Image, [f32; 3]) {
        let (w, h) = (240, 160);
        let air = [0.8, 0.82, 0.85];
        let mut clear = Image::new(w, h);
        let mut hazy = Image::new(w, h);
        for i in 0..w * h {
            let (x, y) = (i % w, i / w);
            let checker = if (x / 6 + y / 6) % 2 == 0 { 0.05 } else { 0.35 };
            let px = [checker, checker * 0.8, checker * 0.5];
            // Thick enough at the top to be nothing but air, as a sky is.
            let t = (1.2 * y as f32 / h as f32 - 0.1).clamp(0.0, 1.0);
            clear.px_mut()[i] = px;
            hazy.px_mut()[i] = std::array::from_fn(|c| px[c] * t + air[c] * (1.0 - t));
        }
        (clear, hazy, air)
    }

    fn error(a: &Image, b: &Image) -> f32 {
        (a.d.iter().zip(&b.d).map(|(x, y)| (x - y).powi(2)).sum::<f32>() / a.d.len() as f32).sqrt()
    }

    #[test]
    fn the_minimum_filter_is_a_minimum() {
        let mut p = Plane::new(13, 7);
        for (i, v) in p.d.iter_mut().enumerate() {
            *v = ((i * 7919) % 31) as f32;
        }
        for r in [1, 2, 4] {
            let fast = min_filter(&p, r);
            for y in 0..7usize {
                for x in 0..13usize {
                    let mut m = f32::MAX;
                    for yy in y.saturating_sub(r)..=(y + r).min(6) {
                        for xx in x.saturating_sub(r)..=(x + r).min(12) {
                            m = m.min(p.d[yy * 13 + xx]);
                        }
                    }
                    assert_eq!(fast.d[y * 13 + x], m, "r {r} at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn the_air_is_found() {
        let (_, hazy, air) = scene();
        let found = air_light(&hazy);
        for c in 0..3 {
            assert!((found[c] - air[c]).abs() < 0.15, "air {found:?}, wanted {air:?}");
        }
    }

    #[test]
    fn haze_comes_out() {
        let (clear, hazy, air) = scene();
        let mut out = hazy.clone();
        apply(&mut out, air, 1.0, 240);
        // Below the top third, where there is still a scene behind the haze.
        let lower = |img: &Image| img.crop(0, 60, 240, 100);
        let (before, after) = (error(&lower(&hazy), &lower(&clear)), error(&lower(&out), &lower(&clear)));
        assert!(after < before * 0.6, "{before} -> {after}");
    }

    #[test]
    fn haze_goes_in_and_nothing_is_nothing() {
        let (clear, hazy, air) = scene();
        let mut same = hazy.clone();
        apply(&mut same, air, 0.0, 240);
        assert_eq!(same.d, hazy.d);
        let mut more = hazy.clone();
        apply(&mut more, air, -1.0, 240);
        assert!(error(&more, &clear) > error(&hazy, &clear));
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    /// numpy's percentile interpolates linearly between order statistics; a
    /// naive "nearest rank" would move every black point and noise floor.
    #[test]
    fn percentile_matches_numpy() {
        let a = [0.0f32, 1.0, 2.0, 3.0, 4.0];
        for (q, want) in [(0.0, 0.0), (8.0, 0.32), (25.0, 1.0), (50.0, 2.0), (80.0, 3.2),
                          (100.0, 4.0)] {
            assert!((percentile(&a, q) - want).abs() < 1e-5, "q={} got {}", q, percentile(&a, q));
        }
    }

    /// Pillow scales the triangle filter by the downscale factor, so a plain
    /// 2x2 bilinear tap would alias.
    #[test]
    fn resize_matches_pillow() {
        let mut p = Plane::new(8, 8);
        for i in 0..64 {
            p.d[i] = i as f32;
        }
        let out = resize_plane(&p, 3, 3);
        let want = [9.947369, 12.342100, 14.736840,
                    29.105261, 31.500000, 33.894741,
                    48.263161, 50.657902, 53.052631];
        for (got, want) in out.d.iter().zip(want.iter()) {
            assert!((got - want).abs() < 1e-3, "got {:?} want {:?}", out.d, want);
        }
    }

    #[test]
    fn oklab_matches_reference() {
        let (l, a, b) = rgb_to_oklab_px(0.2, 0.5, 0.8);
        assert!((l - 0.766887).abs() < 1e-5);
        assert!((a + 0.046271).abs() < 1e-5);
        assert!((b + 0.077996).abs() < 1e-5);
        let (r, g, bb) = oklab_to_rgb_px(l, a, b);
        assert!((r - 0.2).abs() < 1e-4 && (g - 0.5).abs() < 1e-4 && (bb - 0.8).abs() < 1e-4);
    }

    #[test]
    fn box_blur_normalises_at_the_border() {
        let mut p = Plane::filled(9, 9, 1.0);
        let out = box_blur(&p, 2);
        for v in out.d.iter() {
            assert!((v - 1.0).abs() < 1e-6, "border not normalised: {}", v);
        }
        p.d[40] = 10.0;
        let out = box_blur(&p, 1);
        assert!(out.at(4, 4) > 1.0);
    }
}

#[cfg(test)]
mod fast_tests {
    use crate::*;

    /// The fast versions stand in for the exact ones everywhere, so they are
    /// held to them here, over the whole range each one is used on.
    #[test]
    fn fast_cbrt_is_float_precise() {
        let mut worst = 0f32;
        for i in -20000..=20000 {
            let x = i as f32 / 997.0 * 3.3;
            if x == 0.0 { continue; }
            worst = worst.max(((cbrt(x) - x.cbrt()) / x.cbrt()).abs());
        }
        for x in [1e-12f32, 1e-6, 0.0031, 7.0, 1e6] {
            worst = worst.max(((cbrt(x) - x.cbrt()) / x.cbrt()).abs());
        }
        assert!(worst < 1e-6, "cbrt relative error {worst}");
        assert_eq!(cbrt(0.0), 0.0);
    }

    #[test]
    fn fast_log2_and_exp2_are_within_a_millionth() {
        let (mut lg, mut ex) = (0f32, 0f32);
        for i in 1..=400_000 {
            let x = i as f32 / 40_000.0;   // 2.5e-5 .. 10
            lg = lg.max((fast_log2(x) - x.log2()).abs());
            let e = (i as f32 - 200_000.0) / 10_000.0;   // -20 .. 20
            ex = ex.max(((fast_exp2(e) - e.exp2()) / e.exp2()).abs());
        }
        assert!(lg < 2e-6, "log2 error {lg}");
        assert!(ex < 2e-6, "exp2 relative error {ex}");
    }

    #[test]
    fn fast_atan2_is_within_a_ten_thousandth() {
        let mut worst = 0f32;
        for i in 0..3600 {
            let angle = (i as f32 / 10.0).to_radians() - std::f32::consts::PI;
            for r in [1e-4f32, 0.05, 1.0, 30.0] {
                let (y, x) = (angle.sin() * r, angle.cos() * r);
                let mut d = (fast_atan2(y, x) - y.atan2(x)).abs();
                if d > std::f32::consts::PI { d = std::f32::consts::TAU - d; }
                worst = worst.max(d);
            }
        }
        assert!(worst < 1e-4, "atan2 error {worst} rad");
    }

    /// Below what a 16-bit export can hold, 1/65535, with room to spare.
    #[test]
    fn transfer_tables_match_the_curves() {
        let (mut enc, mut dec) = (0f32, 0f32);
        for i in 0..=200_000 {
            let x = i as f32 / 200_000.0;
            enc = enc.max((srgb_encode_scalar(x) - srgb_encode_exact(x)).abs());
            dec = dec.max((srgb_decode_scalar(x) - srgb_decode_exact(x)).abs());
        }
        assert!(enc < 2e-6, "encode error {enc}");
        assert!(dec < 2e-6, "decode error {dec}");
    }

    /// The rewritten blur against the definition: the mean of every pixel
    /// within r, cut short at the edges, at every position including them.
    #[test]
    fn box_blur_is_the_windowed_mean() {
        let (w, h) = (37usize, 23usize);
        let mut p = Plane::new(w, h);
        for (i, v) in p.d.iter_mut().enumerate() {
            *v = ((i * 7919) % 101) as f32 / 101.0;
        }
        for r in [1usize, 2, 5, 30] {
            let got = box_blur(&p, r);
            for y in 0..h {
                for x in 0..w {
                    let (x0, x1) = (x.saturating_sub(r), (x + r + 1).min(w));
                    let (y0, y1) = (y.saturating_sub(r), (y + r + 1).min(h));
                    let mut s = 0.0f64;
                    for yy in y0..y1 { for xx in x0..x1 { s += p.d[yy * w + xx] as f64; } }
                    let want = (s / ((x1 - x0) * (y1 - y0)) as f64) as f32;
                    assert!((got.d[y * w + x] - want).abs() < 1e-5, "r={r} at {x},{y}");
                }
            }
        }
    }
}

#[cfg(test)]
mod matrix_tests {
    use crate::*;

    /// Grey card maths can only catch a wrong constant by luck; this holds
    /// the product to the two published matrices it is made of.
    #[test]
    fn the_cone_matrix_is_bradford_over_srgb() {
        let bradford = Matrix3([[0.8951f32, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367],
                                [0.0389, -0.0685, 1.0296]]);
        let product = bradford.mul(&SRGB_TO_XYZ);
        for i in 0..3 {
            for j in 0..3 {
                assert!((product[i][j] - SRGB_TO_BRADFORD[i][j]).abs() < 1e-5, "[{i}][{j}]");
            }
        }
    }

    #[test]
    fn inverse_undoes_and_refuses_the_singular() {
        let m = Mat3d::IDENTITY.mul(&Matrix3([[2.0, 1.0, 0.0], [0.0, 1.0, 3.0], [1.0, 0.0, 1.0]]));
        let back = m.mul(&m.inverse(1e-12).unwrap());
        for i in 0..3 { for j in 0..3 {
            assert!((back[i][j] - if i == j { 1.0 } else { 0.0 }).abs() < 1e-12);
        } }
        assert!(Mat3::ZERO.inverse(1e-9).is_none());
    }

    #[test]
    fn adaptation_takes_white_to_neutral() {
        let white = [1.2f32, 1.0, 0.7];
        let out = bradford_adaptation(white).apply(white);
        for v in out { assert!((v - 1.0).abs() < 1e-5, "{out:?}"); }
    }
}


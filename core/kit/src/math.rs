//! Arithmetic the pipeline does per pixel, made cheap enough to.

/// `log2` of a positive number to about a millionth, without the software
/// routine.
#[inline]
pub fn fast_log2(x: f32) -> f32 {
    let bits = x.max(f32::MIN_POSITIVE).to_bits();
    let exponent = ((bits >> 23) & 0xff) as i32 - 127;
    let m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000);
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let ln = 2.0 * t * (1.0 + t2 * (1.0 / 3.0 + t2 * (1.0 / 5.0 + t2 * (1.0 / 7.0 + t2 * (1.0 / 9.0)))));
    exponent as f32 + ln * std::f32::consts::LOG2_E
}

/// `exp2` to about a millionth, the same way round: the integer part goes
/// straight into the exponent bits, the fraction through a short series.
#[inline]
pub fn fast_exp2(x: f32) -> f32 {
    let x = x.clamp(-126.0, 127.0);
    let i = x.floor();
    let f = (x - i) * std::f32::consts::LN_2;
    let p = 1.0 + f * (1.0 + f * (0.5 + f * (1.0 / 6.0 + f * (1.0 / 24.0 + f * (1.0 / 120.0
        + f * (1.0 / 720.0 + f * (1.0 / 5040.0)))))));
    p * f32::from_bits(((i as i32 + 127) as u32) << 23)
}

/// `atan2` to within a ten-thousandth of a radian, without the software
/// routine.
#[inline]
pub fn fast_atan2(y: f32, x: f32) -> f32 {
    use std::f32::consts::{FRAC_PI_2, PI};
    let (ax, ay) = (x.abs(), y.abs());
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let swap = ay > ax;
    let z = if swap { ax / ay } else { ay / ax };
    let z2 = z * z;
    // Minimax on [0, 1].
    let mut a = z * (0.999_866 + z2 * (-0.330_299_5 + z2 * (0.180_141 + z2 * (-0.085_133 + z2 * 0.020_835_1))));
    if swap {
        a = FRAC_PI_2 - a;
    }
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 { -a } else { a }
}

/// Cube root, keeping the sign, to float precision.
#[inline]
pub fn cbrt(x: f32) -> f32 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let a = x.abs();
    let mut y = f32::from_bits(a.to_bits() / 3 + 709_921_077);
    let y3 = y * y * y;
    y *= (y3 + 2.0 * a) / (2.0 * y3 + a);
    let y3 = y * y * y;
    y *= (y3 + 2.0 * a) / (2.0 * y3 + a);
    y.copysign(x)
}

// -------------------------------------------------------------------------
// misc
// -------------------------------------------------------------------------

#[inline]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-9)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// numpy's default ("linear") percentile, on an already-collected sample.
pub fn percentile_sorted(sorted: &[f32], q: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    let pos = (q / 100.0) * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let frac = (pos - lo as f64) as f32;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

pub fn percentiles(values: &[f32], qs: &[f64]) -> Vec<f32> {
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    qs.iter().map(|q| percentile_sorted(&v, *q)).collect()
}

pub fn percentile(values: &[f32], q: f64) -> f32 {
    percentiles(values, &[q])[0]
}

pub fn mean(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|v| *v as f64).sum::<f64>() / values.len() as f64) as f32
}

pub fn std_dev(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let m = mean(values) as f64;
    let var = values.iter().map(|v| (*v as f64 - m).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt() as f32
}

pub fn median(values: &[f32]) -> f32 {
    percentile(values, 50.0)
}

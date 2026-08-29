//! Low-level image primitives: resampling, edge-aware blur, colour spaces.

#[derive(Clone, Debug)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>,
}

impl Plane {
    pub fn new(w: usize, h: usize) -> Self {
        Plane { w, h, d: vec![0.0; w * h] }
    }
    pub fn filled(w: usize, h: usize, v: f32) -> Self {
        Plane { w, h, d: vec![v; w * h] }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.d[y * self.w + x]
    }
    pub fn len(&self) -> usize {
        self.d.len()
    }
    /// Sub-rectangle copy (used by `stats_view` and centre crops).
    pub fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Plane {
        let mut out = Plane::new(w, h);
        for y in 0..h {
            let src = (y0 + y) * self.w + x0;
            out.d[y * w..(y + 1) * w].copy_from_slice(&self.d[src..src + w]);
        }
        out
    }
}

/// Interleaved RGB, linear light.
#[derive(Clone, Debug)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>, // len = w*h*3
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Image { w, h, d: vec![0.0; w * h * 3] }
    }
    pub fn plane(&self, c: usize) -> Plane {
        let mut p = Plane::new(self.w, self.h);
        for i in 0..self.w * self.h {
            p.d[i] = self.d[i * 3 + c];
        }
        p
    }
    pub fn set_plane(&mut self, c: usize, p: &Plane) {
        for i in 0..self.w * self.h {
            self.d[i * 3 + c] = p.d[i];
        }
    }
    pub fn from_planes(r: &Plane, g: &Plane, b: &Plane) -> Image {
        let mut img = Image::new(r.w, r.h);
        for i in 0..r.w * r.h {
            img.d[i * 3] = r.d[i];
            img.d[i * 3 + 1] = g.d[i];
            img.d[i * 3 + 2] = b.d[i];
        }
        img
    }
    pub fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Image {
        let mut out = Image::new(w, h);
        for y in 0..h {
            let src = ((y0 + y) * self.w + x0) * 3;
            out.d[y * w * 3..(y + 1) * w * 3].copy_from_slice(&self.d[src..src + w * 3]);
        }
        out
    }
    pub fn scale_channels(&mut self, g: [f32; 3]) {
        for px in self.d.chunks_exact_mut(3) {
            px[0] *= g[0];
            px[1] *= g[1];
            px[2] *= g[2];
        }
    }
    pub fn scale(&mut self, g: f32) {
        for v in self.d.iter_mut() {
            *v *= g;
        }
    }
}

// -------------------------------------------------------------------------
// resampling, matches Pillow's `Image.resize(..., BILINEAR)`
// -------------------------------------------------------------------------

/// Pillow scales the triangle filter's support by the downscale factor, which
/// is what makes its BILINEAR an antialiasing filter on the way down.
fn coeffs(in_size: usize, out_size: usize) -> (usize, Vec<i32>, Vec<f32>) {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = 1.0 * filterscale; // triangle filter support = 1.0
    let ksize = (support.ceil() * 2.0) as usize + 1;
    let mut bounds = vec![0i32; out_size * 2];
    let mut kk = vec![0f32; out_size * ksize];
    for xx in 0..out_size {
        let center = (xx as f64 + 0.5) * scale;
        let ww_start = (center - support + 0.5).floor().max(0.0) as usize;
        let mut xmax = (center + support + 0.5).floor() as i64;
        if xmax > in_size as i64 {
            xmax = in_size as i64;
        }
        let xmin = ww_start;
        let count = (xmax - xmin as i64).max(0) as usize;
        let mut sum = 0.0f64;
        for i in 0..count {
            let x = (xmin + i) as f64 - center + 0.5;
            let t = (x / filterscale).abs();
            let w = if t < 1.0 { 1.0 - t } else { 0.0 };
            kk[xx * ksize + i] = w as f32;
            sum += w;
        }
        if sum != 0.0 {
            for i in 0..count {
                kk[xx * ksize + i] = (kk[xx * ksize + i] as f64 / sum) as f32;
            }
        }
        bounds[xx * 2] = xmin as i32;
        bounds[xx * 2 + 1] = count as i32;
    }
    (ksize, bounds, kk)
}

pub fn resize_plane(p: &Plane, w: usize, h: usize) -> Plane {
    if p.w == w && p.h == h {
        return p.clone();
    }
    // horizontal pass
    let (ks, bounds, kk) = coeffs(p.w, w);
    let mut tmp = Plane::new(w, p.h);
    for y in 0..p.h {
        let row = y * p.w;
        for x in 0..w {
            let xmin = bounds[x * 2] as usize;
            let n = bounds[x * 2 + 1] as usize;
            let mut acc = 0.0f32;
            for i in 0..n {
                acc += p.d[row + xmin + i] * kk[x * ks + i];
            }
            tmp.d[y * w + x] = acc;
        }
    }
    // vertical pass: a whole source row at a time, weighted into the output
    // row, so the inner loop is contiguous (and vectorised) instead of striding
    // down a column per output pixel.
    let (ks2, bounds2, kk2) = coeffs(p.h, h);
    let mut out = Plane::new(w, h);
    for y in 0..h {
        let ymin = bounds2[y * 2] as usize;
        let n = bounds2[y * 2 + 1] as usize;
        let dst = &mut out.d[y * w..(y + 1) * w];
        for i in 0..n {
            let k = kk2[y * ks2 + i];
            let src = &tmp.d[(ymin + i) * w..(ymin + i + 1) * w];
            for (o, s) in dst.iter_mut().zip(src) {
                *o += *s * k;
            }
        }
    }
    out
}

pub fn resize_rgb(img: &Image, w: usize, h: usize) -> Image {
    if img.w == w && img.h == h {
        return img.clone();
    }
    let r = resize_plane(&img.plane(0), w, h);
    let g = resize_plane(&img.plane(1), w, h);
    let b = resize_plane(&img.plane(2), w, h);
    Image::from_planes(&r, &g, &b)
}

/// Small copy used for every statistic, so analysis cost is resolution
/// independent and single hot pixels cannot swing a percentile.
pub fn thumbnail(img: &Image, long_edge: usize) -> Image {
    let scale = long_edge as f64 / img.w.max(img.h) as f64;
    if scale >= 1.0 {
        return img.clone();
    }
    let w = ((img.w as f64 * scale).round() as usize).max(1);
    let h = ((img.h as f64 * scale).round() as usize).max(1);
    resize_rgb(img, w, h)
}

// -------------------------------------------------------------------------
// blurs
// -------------------------------------------------------------------------

/// Separable box blur, normalised at the borders.
pub fn box_blur(p: &Plane, r: usize) -> Plane {
    if r < 1 {
        return p.clone();
    }
    let (w, h) = (p.w, p.h);
    let mut tmp = Plane::new(w, h);
    // rows: a sliding window along each row
    let inv: Vec<f32> = (0..=2 * r + 1).map(|n| if n > 0 { 1.0 / n as f32 } else { 0.0 }).collect();
    for y in 0..h {
        let row = &p.d[y * w..(y + 1) * w];
        let out = &mut tmp.d[y * w..(y + 1) * w];
        let mut sum = 0.0f32;
        for v in row.iter().take(r.min(w)) {
            sum += *v;
        }
        for x in 0..w {
            if x + r < w {
                sum += row[x + r];
            }
            if x > r {
                sum -= row[x - r - 1];
            }
            let lo = x.saturating_sub(r);
            let hi = (x + r + 1).min(w);
            out[x] = sum * inv[hi - lo];
        }
    }
    // columns: one running sum per column, slid down the frame row by row
    let mut out = Plane::new(w, h);
    let mut sum = vec![0f32; w];
    for y in 0..r.min(h) {
        for (s, v) in sum.iter_mut().zip(&tmp.d[y * w..(y + 1) * w]) {
            *s += *v;
        }
    }
    for y in 0..h {
        if y + r < h {
            for (s, v) in sum.iter_mut().zip(&tmp.d[(y + r) * w..(y + r + 1) * w]) {
                *s += *v;
            }
        }
        if y > r {
            for (s, v) in sum.iter_mut().zip(&tmp.d[(y - r - 1) * w..(y - r) * w]) {
                *s -= *v;
            }
        }
        let k = inv[(y + r + 1).min(h) - y.saturating_sub(r)];
        for (o, s) in out.d[y * w..(y + 1) * w].iter_mut().zip(&sum) {
            *o = *s * k;
        }
    }
    out
}

/// Three box passes approximate a Gaussian closely enough for photo work.
pub fn gaussian_blur(p: &Plane, sigma: f32) -> Plane {
    if sigma <= 0.0 {
        return p.clone();
    }
    let r = ((sigma * 1.5).round() as usize).max(1);
    let mut out = box_blur(p, r);
    out = box_blur(&out, r);
    box_blur(&out, r)
}

/// Fast guided filter (He et al.).
pub fn guided_filter(guide: &Plane, src: &Plane, radius: usize, eps: f32, subsample: usize) -> Plane {
    let (w, h) = (guide.w, guide.h);
    let s = subsample.max(1);
    let sw = (w / s).max(8);
    let sh = (h / s).max(8);
    let r = (radius / s).max(1);

    let gi = resize_plane(guide, sw, sh);
    let pi = resize_plane(src, sw, sh);

    let mut gg = Plane::new(sw, sh);
    let mut gp = Plane::new(sw, sh);
    for i in 0..gi.d.len() {
        gg.d[i] = gi.d[i] * gi.d[i];
        gp.d[i] = gi.d[i] * pi.d[i];
    }
    let mean_i = box_blur(&gi, r);
    let mean_p = box_blur(&pi, r);
    let mean_gg = box_blur(&gg, r);
    let mean_gp = box_blur(&gp, r);

    let mut a = Plane::new(sw, sh);
    let mut b = Plane::new(sw, sh);
    for i in 0..a.d.len() {
        let var_i = mean_gg.d[i] - mean_i.d[i] * mean_i.d[i];
        let cov = mean_gp.d[i] - mean_i.d[i] * mean_p.d[i];
        a.d[i] = cov / (var_i + eps);
        b.d[i] = mean_p.d[i] - a.d[i] * mean_i.d[i];
    }
    let a = resize_plane(&box_blur(&a, r), w, h);
    let b = resize_plane(&box_blur(&b, r), w, h);

    let mut out = Plane::new(w, h);
    for i in 0..out.d.len() {
        out.d[i] = a.d[i] * guide.d[i] + b.d[i];
    }
    out
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

/// Rec.709 luminance of linear RGB.
pub fn luminance(img: &Image) -> Plane {
    let mut p = Plane::new(img.w, img.h);
    for i in 0..img.w * img.h {
        p.d[i] = 0.2126 * img.d[i * 3] + 0.7152 * img.d[i * 3 + 1] + 0.0722 * img.d[i * 3 + 2];
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

/// Bilinear sample with edge clamping, the sampler used by every resample
/// (lens geometry, framing, radial scaling).
#[inline]
pub fn sample_bilinear(p: &Plane, x: f32, y: f32) -> f32 {
    let w = p.w;
    let h = p.h;
    let x = x.clamp(0.0, w as f32 - 1.001);
    let y = y.clamp(0.0, h as f32 - 1.001);
    let x0 = x as usize;
    let y0 = y as usize;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let top = p.d[y0 * w + x0] * (1.0 - fx) + p.d[y0 * w + x1] * fx;
    let bot = p.d[y1 * w + x0] * (1.0 - fx) + p.d[y1 * w + x1] * fx;
    top * (1.0 - fy) + bot * fy
}

#[cfg(test)]
mod tests {
    use super::*;

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
    use super::*;

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

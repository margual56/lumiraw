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
    // vertical pass
    let (ks2, bounds2, kk2) = coeffs(p.h, h);
    let mut out = Plane::new(w, h);
    for y in 0..h {
        let ymin = bounds2[y * 2] as usize;
        let n = bounds2[y * 2 + 1] as usize;
        for x in 0..w {
            let mut acc = 0.0f32;
            for i in 0..n {
                acc += tmp.d[(ymin + i) * w + x] * kk2[y * ks2 + i];
            }
            out.d[y * w + x] = acc;
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

/// Separable box blur, normalised at the borders (O(n) via a running sum).
pub fn box_blur(p: &Plane, r: usize) -> Plane {
    if r < 1 {
        return p.clone();
    }
    let mut tmp = Plane::new(p.w, p.h);
    // rows
    for y in 0..p.h {
        let row = &p.d[y * p.w..(y + 1) * p.w];
        let mut cs = vec![0f32; p.w + 1];
        for x in 0..p.w {
            cs[x + 1] = cs[x] + row[x];
        }
        for x in 0..p.w {
            let lo = x.saturating_sub(r);
            let hi = (x + r + 1).min(p.w);
            tmp.d[y * p.w + x] = (cs[hi] - cs[lo]) / (hi - lo) as f32;
        }
    }
    // columns
    let mut out = Plane::new(p.w, p.h);
    let mut cs = vec![0f32; p.h + 1];
    for x in 0..p.w {
        for y in 0..p.h {
            cs[y + 1] = cs[y] + tmp.d[y * p.w + x];
        }
        for y in 0..p.h {
            let lo = y.saturating_sub(r);
            let hi = (y + r + 1).min(p.h);
            out.d[y * p.w + x] = (cs[hi] - cs[lo]) / (hi - lo) as f32;
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

#[inline]
pub fn srgb_encode_scalar(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
pub fn srgb_decode_scalar(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
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

#[inline]
fn cbrt(x: f32) -> f32 {
    // numpy's cbrt keeps the sign; f32::cbrt does too.
    x.cbrt()
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

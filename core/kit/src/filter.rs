//! Resampling and blurs: the operations that read a neighbourhood rather than a
//! pixel.

use crate::image::{Image, Plane};

// Resampling, matching Pillow's `Image.resize(..., BILINEAR)`.
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
    let mut out = Plane::new(w, h);
    out.d = resize::<1>(&p.d, p.w, p.h, w, h);
    out
}

pub fn resize_rgb(img: &Image, w: usize, h: usize) -> Image {
    if img.w == w && img.h == h {
        return img.clone();
    }
    let mut out = Image::new(w, h);
    out.d = resize::<3>(&img.d, img.w, img.h, w, h);
    out
}

/// Resample `C` interleaved channels, horizontally then vertically.
fn resize<const C: usize>(src: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    let (ks, bounds, kk) = coeffs(sw, w);
    let mut tmp = vec![0f32; w * sh * C];
    for (dst, row) in tmp.chunks_exact_mut(w * C).zip(src.chunks_exact(sw * C)) {
        for x in 0..w {
            let xmin = bounds[x * 2] as usize;
            let n = bounds[x * 2 + 1] as usize;
            let mut acc = [0f32; C];
            for i in 0..n {
                let k = kk[x * ks + i];
                let px = &row[(xmin + i) * C..(xmin + i + 1) * C];
                for c in 0..C {
                    acc[c] += px[c] * k;
                }
            }
            dst[x * C..(x + 1) * C].copy_from_slice(&acc);
        }
    }
    // A whole source row at a time, weighted into the output row, so the
    // inner loop runs along memory instead of down a column.
    let (ks, bounds, kk) = coeffs(sh, h);
    let stride = w * C;
    let mut out = vec![0f32; stride * h];
    for (y, dst) in out.chunks_exact_mut(stride).enumerate() {
        let ymin = bounds[y * 2] as usize;
        let n = bounds[y * 2 + 1] as usize;
        for i in 0..n {
            let k = kk[y * ks + i];
            let row = &tmp[(ymin + i) * stride..(ymin + i + 1) * stride];
            for (o, s) in dst.iter_mut().zip(row) {
                *o += *s * k;
            }
        }
    }
    out
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

// Blurs
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
    let [out] = guided_filter_each(guide, [src], radius, eps, subsample);
    out
}

/// `guided_filter` on several planes under one guide, whose statistics are
/// worked out once for all of them.
pub fn guided_filter_each<const N: usize>(guide: &Plane, srcs: [&Plane; N], radius: usize,
                                          eps: f32, subsample: usize) -> [Plane; N] {
    let (w, h) = (guide.w, guide.h);
    let s = subsample.max(1);
    let sw = (w / s).max(8);
    let sh = (h / s).max(8);
    let r = (radius / s).max(1);

    let gi = resize_plane(guide, sw, sh);
    let mut gg = Plane::new(sw, sh);
    for i in 0..gi.d.len() {
        gg.d[i] = gi.d[i] * gi.d[i];
    }
    let mean_i = box_blur(&gi, r);
    let mean_gg = box_blur(&gg, r);

    srcs.map(|src| {
        let pi = resize_plane(src, sw, sh);
        let mut gp = Plane::new(sw, sh);
        for i in 0..gi.d.len() {
            gp.d[i] = gi.d[i] * pi.d[i];
        }
        let mean_p = box_blur(&pi, r);
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
    })
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

/// `sample_bilinear` on all three channels of an interleaved frame at once,
/// with the same arithmetic, so there is no need to split it into planes.
pub fn sample_bilinear_rgb(img: &Image, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (img.w, img.h);
    let x = x.clamp(0.0, w as f32 - 1.001);
    let y = y.clamp(0.0, h as f32 - 1.001);
    let x0 = x as usize;
    let y0 = y as usize;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let px = img.px();
    let (a, b, c, d) = (px[y0 * w + x0], px[y0 * w + x1], px[y1 * w + x0], px[y1 * w + x1]);
    std::array::from_fn(|k| {
        let top = a[k] * (1.0 - fx) + b[k] * fx;
        let bot = c[k] * (1.0 - fx) + d[k] * fx;
        top * (1.0 - fy) + bot * fy
    })
}

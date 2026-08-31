//! Framing: straightening, perspective and crop.

use kit::{sample_bilinear_rgb, Image, Mat3d, Matrix3, Plane};
use std::borrow::Cow;

pub const MAX_CANVAS_GROWTH: f64 = 2.4;

pub type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framing {
    pub angle: f64,
    pub perspective_v: f64,
    pub perspective_h: f64,
    pub crop: Option<Rect>,
    pub auto_fit: bool,
}

impl Default for Framing {
    fn default() -> Self {
        Framing { angle: 0.0, perspective_v: 0.0, perspective_h: 0.0, crop: None, auto_fit: true }
    }
}

impl Framing {
    pub fn transforms(&self) -> bool {
        self.angle.abs() >= 1e-3
            || self.perspective_v.abs() >= 1e-4
            || self.perspective_h.abs() >= 1e-4
    }
    pub fn is_identity(&self) -> bool {
        !self.transforms() && self.crop.is_none()
    }
    pub fn key(&self) -> String {
        format!(
            "{:.3}|{:.4}|{:.4}|{}|{}",
            self.angle,
            self.perspective_v,
            self.perspective_h,
            self.auto_fit,
            match self.crop {
                None => "none".to_string(),
                Some(c) => format!("{:.5},{:.5},{:.5},{:.5}", c.0, c.1, c.2, c.3),
            }
        )
    }
}


/// Source-centred pixels -> canvas-centred pixels.
fn homography(w: usize, h: usize, f: &Framing) -> Mat3d {
    let half_diag = ((w * w + h * h) as f64).sqrt() / 2.0;
    let mut persp = Mat3d::IDENTITY;
    persp.0[2][0] = f.perspective_h / half_diag;
    persp.0[2][1] = f.perspective_v / half_diag;

    let theta = f.angle.to_radians();
    let rot = Matrix3([
        [theta.cos(), -theta.sin(), 0.0],
        [theta.sin(), theta.cos(), 0.0],
        [0.0, 0.0, 1.0],
    ]);
    rot.mul(&persp)
}


/// How big the transformed frame is, before cropping.  Returns (h, w).
pub fn canvas_size(w: usize, h: usize, f: &Framing) -> (usize, usize) {
    if f.is_identity() {
        return (h, w);
    }
    let m = homography(w, h, f);
    let corners = [
        [-(w as f64) / 2.0, -(h as f64) / 2.0],
        [w as f64 / 2.0, -(h as f64) / 2.0],
        [w as f64 / 2.0, h as f64 / 2.0],
        [-(w as f64) / 2.0, h as f64 / 2.0],
    ];
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for c in corners.iter() {
        let z = m[2][0] * c[0] + m[2][1] * c[1] + m[2][2];
        let denom = z.abs().max(1e-9) * z.signum();
        let x = (m[0][0] * c[0] + m[0][1] * c[1] + m[0][2]) / denom;
        let y = (m[1][0] * c[0] + m[1][1] * c[1] + m[1][2]) / denom;
        xmin = xmin.min(x);
        xmax = xmax.max(x);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
    }
    let cw = xmax - xmin;
    let ch = ymax - ymin;
    (
        (ch.min(h as f64 * MAX_CANVAS_GROWTH).round() as usize).max(8),
        (cw.min(w as f64 * MAX_CANVAS_GROWTH).round() as usize).max(8),
    )
}

/// The crop actually used: yours if you drew one, otherwise the largest
/// empty-corner-free rectangle when auto-fit is on.
pub fn effective_crop(w: usize, h: usize, f: &Framing) -> Option<Rect> {
    if f.crop.is_some() {
        return f.crop;
    }
    if f.auto_fit && f.transforms() {
        return Some(cover_crop(w, h, f));
    }
    None
}

pub fn apply(img: &Image, f: &Framing, crop: bool) -> Image {
    if f.is_identity() {
        return img.clone();
    }
    let (w, h) = (img.w, img.h);
    let box_ = if crop { effective_crop(w, h, f) } else { None };
    if crop && box_.is_none() && !f.transforms() {
        return img.clone();
    }
    let (ch, cw) = canvas_size(w, h, f);
    let inv = homography(w, h, f).inverse(1e-15).unwrap_or(Mat3d::ZERO);

    let (mut x0, mut y0) = (0.0f64, 0.0f64);
    let (mut out_w, mut out_h) = (cw, ch);
    if let Some((cx, cy, cwf, chf)) = box_ {
        x0 = cx as f64 * cw as f64;
        y0 = cy as f64 * ch as f64;
        out_w = ((cwf as f64 * cw as f64).round() as usize).max(8);
        out_h = ((chf as f64 * ch as f64).round() as usize).max(8);
    }

    let mut out = Image::new(out_w, out_h);
    let dst = out.px_mut();
    for oy in 0..out_h {
        let gy = (oy as f64 + y0) - ch as f64 / 2.0;
        for ox in 0..out_w {
            let gx = (ox as f64 + x0) - cw as f64 / 2.0;
            let (sx, sy) = to_source(&inv, gx, gy, w, h);
            if inside(sx, sy, w, h) {
                dst[oy * out_w + ox] = sample_bilinear_rgb(img, sx as f32, sy as f32);
            }
        }
    }
    out
}

/// Where a point of the output canvas, measured from its centre, comes from
/// in the source frame, in the source's pixel coordinates.
#[inline]
fn to_source(inv: &Mat3d, gx: f64, gy: f64, w: usize, h: usize) -> (f64, f64) {
    let mut den = inv[2][0] * gx + inv[2][1] * gy + inv[2][2];
    if den.abs() < 1e-9 {
        den = 1e-9;
    }
    let sx = (inv[0][0] * gx + inv[0][1] * gy + inv[0][2]) / den + (w as f64 - 1.0) / 2.0;
    let sy = (inv[1][0] * gx + inv[1][1] * gy + inv[1][2]) / den + (h as f64 - 1.0) / 2.0;
    (sx, sy)
}

#[inline]
fn inside(sx: f64, sy: f64, w: usize, h: usize) -> bool {
    sx >= 0.0 && sx <= w as f64 - 1.0 && sy >= 0.0 && sy <= h as f64 - 1.0
}

/// The largest centred crop with no empty corners.  Closed form for the
/// rotation, then bisection-free shrinking for the perspective.
pub fn cover_crop(w: usize, h: usize, f: &Framing) -> Rect {
    let (ch, cw) = canvas_size(w, h, f);
    let mut theta = f.angle.to_radians().abs() % std::f64::consts::PI;
    if theta > std::f64::consts::FRAC_PI_2 {
        theta = std::f64::consts::PI - theta;
    }
    let (wf, hf) = (w as f64, h as f64);
    let (rect_w, rect_h);
    if theta < 1e-4 {
        rect_w = wf;
        rect_h = hf;
    } else {
        let (sin_a, cos_a) = (theta.sin(), theta.cos());
        let long_side = wf.max(hf);
        let short_side = wf.min(hf);
        if short_side <= 2.0 * sin_a * cos_a * long_side || (sin_a - cos_a).abs() < 1e-9 {
            let half = 0.5 * short_side;
            if wf >= hf {
                rect_w = half / sin_a;
                rect_h = half / cos_a;
            } else {
                rect_w = half / cos_a;
                rect_h = half / sin_a;
            }
        } else {
            let cos_2a = cos_a * cos_a - sin_a * sin_a;
            rect_w = (wf * cos_a - hf * sin_a) / cos_2a;
            rect_h = (hf * cos_a - wf * sin_a) / cos_2a;
        }
    }

    // A pixel of margin: bilinear sampling on the very edge picks up the void.
    let mut fw = (((rect_w - 2.0) / cw as f64) as f32).clamp(0.05, 1.0);
    let mut fh = (((rect_h - 2.0) / ch as f64) as f32).clamp(0.05, 1.0);

    if f.perspective_v.abs() > 1e-4 || f.perspective_h.abs() > 1e-4 {
        let inv = homography(w, h, f).inverse(1e-15).unwrap_or(Mat3d::ZERO);
        for _ in 0..24 {
            if corners_inside(&inv, w, h, cw, ch, fw, fh) {
                break;
            }
            fw *= 0.96;
            fh *= 0.96;
        }
    }
    ((1.0 - fw) / 2.0, (1.0 - fh) / 2.0, fw, fh)
}

fn corners_inside(inv: &Mat3d, w: usize, h: usize, cw: usize, ch: usize, fw: f32, fh: f32) -> bool {
    let xs = [-fw as f64, fw as f64, fw as f64, -fw as f64];
    let ys = [-fh as f64, -fh as f64, fh as f64, fh as f64];
    for i in 0..4 {
        let (sx, sy) = to_source(inv, xs[i] * cw as f64 / 2.0, ys[i] * ch as f64 / 2.0, w, h);
        if !inside(sx, sy, w, h) {
            return false;
        }
    }
    true
}

/// The sub-rectangle every estimator should measure (see `grade::stats_view`).
pub fn view_bounds(w: usize, h: usize, rect: Option<Rect>) -> Option<(usize, usize, usize, usize)> {
    let (rx, ry, rw, rh) = rect?;
    let x0 = (rx.clamp(0.0, 1.0) * w as f32) as usize;
    let y0 = (ry.clamp(0.0, 1.0) * h as f32) as usize;
    let x1 = ((rx + rw).clamp(0.0, 1.0) * w as f32) as usize;
    let y1 = ((ry + rh).clamp(0.0, 1.0) * h as f32) as usize;
    if x1.saturating_sub(x0) < 16 || y1.saturating_sub(y0) < 16 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// A rectangle drawn in fractions of the kept frame, in fractions of the whole
/// canvas instead.
pub fn in_frame(rect: Rect, frame: Option<Rect>) -> Rect {
    match frame {
        None => rect,
        Some((fx, fy, fw, fh)) => (fx + rect.0 * fw, fy + rect.1 * fh, rect.2 * fw, rect.3 * fh),
    }
}

/// The estimators measure only the rectangle being kept.
pub fn stats_view_img(img: &Image, rect: Option<Rect>) -> Cow<'_, Image> {
    match view_bounds(img.w, img.h, rect) {
        Some((x, y, w, h)) => Cow::Owned(img.crop(x, y, w, h)),
        None => Cow::Borrowed(img),
    }
}

pub fn stats_view_plane(p: &Plane, rect: Option<Rect>) -> Cow<'_, Plane> {
    match view_bounds(p.w, p.h, rect) {
        Some((x, y, w, h)) => Cow::Owned(p.crop(x, y, w, h)),
        None => Cow::Borrowed(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The closed form for the largest empty-corner-free rectangle, checked
    /// against the Python implementation's output for the same frame.
    #[test]
    fn cover_crop_matches_reference() {
        let f = Framing { angle: 8.0, ..Framing::default() };
        let (x, y, w, h) = cover_crop(450, 300, &f);
        for (got, want) in [(x, 0.070694), (y, 0.164027), (w, 0.858613), (h, 0.671947)] {
            assert!((got - want).abs() < 1e-4, "got {} want {}", got, want);
        }
        assert_eq!(canvas_size(450, 300, &f), (360, 487));
    }

    /// Auto-fit must actually remove the void: no sampled pixel inside the
    /// kept rectangle may fall outside the source.
    #[test]
    fn auto_fit_leaves_no_empty_corner() {
        let mut img = Image::new(120, 80);
        for v in img.d.iter_mut() {
            *v = 0.5;
        }
        let f = Framing { angle: 8.0, ..Framing::default() };
        let out = apply(&img, &f, true);
        assert!(out.d.iter().all(|v| *v > 0.4), "black wedge left in the crop");
        // …and without auto-fit there is one, which is what the checkbox is for.
        let f = Framing { angle: 8.0, auto_fit: false, ..Framing::default() };
        let out = apply(&img, &f, true);
        assert!(out.d.iter().any(|v| *v < 0.01));
    }

    #[test]
    fn identity_framing_is_a_no_op() {
        let f = Framing::default();
        assert!(f.is_identity());
        assert_eq!(effective_crop(100, 100, &f), None);
    }
}

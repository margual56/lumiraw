//! Adjustments to part of the picture.

use crate::geometry::Rect;
use crate::ops::{self, Image};
use serde_json::Value;

/// Where a filter reaches.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Full strength on `a`'s side of the line through `a`, fading to nothing
    /// at the parallel line through `b`.
    Linear { a: (f32, f32), b: (f32, f32) },
    /// Full strength inside the ellipse, fading out over `feather` of its
    /// radius beyond it; or the other way round, with `invert`.
    Radial { centre: (f32, f32), radius: (f32, f32), feather: f32, invert: bool },
}

/// One filter: where it reaches and what it does there.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub shape: Shape,
    /// Stops, -3..3.
    pub exposure: f32,
    /// Warmer or cooler, -1..1, on the same scale as the global control.
    pub temperature: f32,
    /// -1 (grey) .. 1 (twice the colour).
    pub saturation: f32,
}

/// One healed spot: the disc at `at`, of radius `r` as a fraction of the
/// frame's long edge, replaced from the disc at `from`.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub at: (f32, f32),
    pub r: f32,
    pub from: Option<(f32, f32)>,
}

fn pair(v: Option<&Value>) -> Option<(f32, f32)> {
    let a = v?.as_array()?;
    Some((a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
}

fn num(v: &Value, k: &str, lo: f32, hi: f32) -> f32 {
    v.get(k).and_then(|x| x.as_f64()).map(|x| (x as f32).clamp(lo, hi)).unwrap_or(0.0)
}

pub fn filters_from_json(v: Option<&Value>) -> Vec<Filter> {
    let Some(list) = v.and_then(|v| v.as_array()) else { return Vec::new() };
    list.iter().filter_map(|f| {
        let shape = match f.get("kind")?.as_str()? {
            "linear" => Shape::Linear { a: pair(f.get("a"))?, b: pair(f.get("b"))? },
            "radial" => {
                let radius = pair(f.get("radius"))?;
                Shape::Radial {
                    centre: pair(f.get("centre"))?,
                    radius: (radius.0.abs().max(1e-3), radius.1.abs().max(1e-3)),
                    feather: f.get("feather").and_then(|x| x.as_f64()).unwrap_or(0.5)
                        .clamp(0.0, 1.0) as f32,
                    invert: f.get("invert").and_then(|x| x.as_bool()).unwrap_or(false),
                }
            }
            _ => return None,
        };
        Some(Filter {
            shape,
            exposure: num(f, "exposure", -3.0, 3.0),
            temperature: num(f, "temperature", -1.0, 1.0),
            saturation: num(f, "saturation", -1.0, 1.0),
        })
    }).collect()
}

pub fn spots_from_json(v: Option<&Value>) -> Vec<Spot> {
    let Some(list) = v.and_then(|v| v.as_array()) else { return Vec::new() };
    list.iter().filter_map(|s| Some(Spot {
        at: pair(s.get("at"))?,
        r: s.get("r").and_then(|x| x.as_f64()).unwrap_or(0.01).clamp(0.001, 0.2) as f32,
        from: pair(s.get("from")),
    })).collect()
}

/// The framed picture's place in this image, in pixels: x, y, width, height.
fn frame_px(img: &Image, frame: Option<Rect>) -> (f32, f32, f32, f32) {
    let (w, h) = (img.w as f32, img.h as f32);
    match frame {
        Some((x, y, fw, fh)) => (x * w, y * h, fw * w, fh * h),
        None => (0.0, 0.0, w, h),
    }
}

/// How strongly a filter applies at a pixel of the framed picture, 0..1.
fn weight(shape: &Shape, u: f32, v: f32, aspect: f32) -> f32 {
    match shape {
        Shape::Linear { a, b } => {
            // Measured in a space where the frame is square in proportion, so
            // the fade runs perpendicular to the lines as drawn.
            let (dx, dy) = ((b.0 - a.0) * aspect, b.1 - a.1);
            let len2 = dx * dx + dy * dy;
            if len2 < 1e-9 {
                return 0.0;
            }
            let t = (((u - a.0) * aspect) * dx + (v - a.1) * dy) / len2;
            1.0 - ops::smoothstep(0.0, 1.0, t)
        }
        Shape::Radial { centre, radius, feather, invert } => {
            let (x, y) = ((u - centre.0) / radius.0, (v - centre.1) / radius.1);
            let d = (x * x + y * y).sqrt();
            let inside = 1.0 - ops::smoothstep(1.0, 1.0 + feather.max(0.02), d);
            if *invert { 1.0 - inside } else { inside }
        }
    }
}

/// The filters, applied to a display-referred frame (see the module notes).
pub fn apply_filters(img: &mut Image, filters: &[Filter], frame: Option<Rect>) {
    if filters.is_empty() {
        return;
    }
    let (fx, fy, fw, fh) = frame_px(img, frame);
    let aspect = fw / fh.max(1.0);
    for y in 0..img.h {
        let v = (y as f32 + 0.5 - fy) / fh;
        for x in 0..img.w {
            let u = (x as f32 + 0.5 - fx) / fw;
            let i = (y * img.w + x) * 3;
            for f in filters {
                let k = weight(&f.shape, u, v, aspect);
                if k <= 1e-4 {
                    continue;
                }
                let px = &mut img.d[i..i + 3];
                if f.exposure != 0.0 {
                    let g = (f.exposure * k).exp2();
                    for c in px.iter_mut() {
                        *c *= g;
                    }
                }
                if f.temperature != 0.0 {
                    // The same gains as the global temperature control, and
                    // the same care to hold luminance while the colour moves.
                    let t = f.temperature * k;
                    let log = [0.35 * t, 0.0, -0.35 * t];
                    let weighted = 0.2126 * log[0] + 0.0722 * log[2];
                    for c in 0..3 {
                        px[c] *= (log[c] - weighted).exp2();
                    }
                }
                if f.saturation != 0.0 {
                    let luma = ops::luminance_px(px);
                    let s = 1.0 + f.saturation * k;
                    for c in px.iter_mut() {
                        *c = luma + (*c - luma) * s;
                    }
                }
            }
        }
    }
}

/// Mean colour of the ring between `r0` and `r1` pixels around a point, which
/// is what a patch has to match to disappear into its surroundings.
fn ring_mean(img: &Image, cx: f32, cy: f32, r0: f32, r1: f32) -> Option<[f32; 3]> {
    let mut sum = [0f64; 3];
    let mut n = 0usize;
    let reach = r1.ceil() as i64;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d < r0 || d > r1 {
                continue;
            }
            let (x, y) = (cx as i64 + dx, cy as i64 + dy);
            if x < 0 || y < 0 || x >= img.w as i64 || y >= img.h as i64 {
                continue;
            }
            let i = (y as usize * img.w + x as usize) * 3;
            for c in 0..3 {
                sum[c] += img.d[i + c] as f64;
            }
            n += 1;
        }
    }
    (n > 0).then(|| [(sum[0] / n as f64) as f32, (sum[1] / n as f64) as f32,
                     (sum[2] / n as f64) as f32])
}

/// Where to take a patch from, when the photographer did not say.
pub fn choose_source(img: &Image, spot: &Spot, frame: Option<Rect>) -> (f32, f32) {
    const JUDGE_EDGE: usize = 800;
    let small = ops::thumbnail(img, JUDGE_EDGE);
    let (fx, fy, fw, fh) = frame_px(&small, frame);
    let long = fw.max(fh);
    let r = (spot.r * long).max(1.5);
    let (cx, cy) = (fx + spot.at.0 * fw, fy + spot.at.1 * fh);
    let Some(target) = ring_mean(&small, cx, cy, r, r * 1.6) else {
        return (spot.at.0 + 2.5 * spot.r * long / fw, spot.at.1);
    };
    let mut best = (f32::MAX, (spot.at.0 + 2.5 * spot.r * long / fw, spot.at.1));
    for k in 0..12 {
        let angle = k as f32 * std::f32::consts::TAU / 12.0;
        let (sx, sy) = (cx + angle.cos() * r * 2.5, cy + angle.sin() * r * 2.5);
        if sx - r < fx || sy - r < fy || sx + r > fx + fw || sy + r > fy + fh {
            continue;
        }
        let Some(ring) = ring_mean(&small, sx, sy, r, r * 1.6) else { continue };
        let Some(inner) = ring_mean(&small, sx, sy, 0.0, r) else { continue };
        // Its surroundings should match ours, and its inside should look like
        // its own surroundings.
        let score: f32 = (0..3).map(|c| (ring[c] - target[c]).abs() + 0.5 * (inner[c] - ring[c]).abs())
            .sum();
        if score < best.0 {
            best = (score, ((sx - fx) / fw, (sy - fy) / fh));
        }
    }
    best.1
}

/// Heal the spots, on a scene-linear frame before anything is measured.
pub fn heal(img: &mut Image, spots: &[Spot], frame: Option<Rect>) {
    if spots.is_empty() {
        return;
    }
    let (fx, fy, fw, fh) = frame_px(img, frame);
    let long = fw.max(fh);
    for spot in spots {
        let from = spot.from.unwrap_or_else(|| choose_source(img, spot, frame));
        let r = (spot.r * long).max(1.0);
        let (tx, ty) = (fx + spot.at.0 * fw, fy + spot.at.1 * fh);
        let (sx, sy) = (fx + from.0 * fw, fy + from.1 * fh);
        let (Some(t_ring), Some(s_ring)) = (ring_mean(img, tx, ty, r, r * 1.5),
                                            ring_mean(img, sx, sy, r, r * 1.5)) else { continue };
        let gain = [t_ring[0] / s_ring[0].max(1e-6), t_ring[1] / s_ring[1].max(1e-6),
                    t_ring[2] / s_ring[2].max(1e-6)];
        let reach = r.ceil() as i64;
        // Read every source pixel before writing any, in case the two discs
        // overlap.
        let mut patch = Vec::with_capacity(((2 * reach + 1) * (2 * reach + 1)) as usize);
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (x, y) = ((sx + dx as f32).round() as i64, (sy + dy as f32).round() as i64);
                let px = if x >= 0 && y >= 0 && (x as usize) < img.w && (y as usize) < img.h {
                    let i = (y as usize * img.w + x as usize) * 3;
                    Some([img.d[i], img.d[i + 1], img.d[i + 2]])
                } else {
                    None
                };
                patch.push(px);
            }
        }
        let mut k = 0;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let src = patch[k];
                k += 1;
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let w = 1.0 - ops::smoothstep(r * 0.66, r, d);
                let (x, y) = ((tx + dx as f32).round() as i64, (ty + dy as f32).round() as i64);
                let (Some(s), true) = (src, w > 0.0 && x >= 0 && y >= 0
                                        && (x as usize) < img.w && (y as usize) < img.h) else { continue };
                let i = (y as usize * img.w + x as usize) * 3;
                for c in 0..3 {
                    img.d[i + c] += (s[c] * gain[c] - img.d[i + c]) * w;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, v: f32) -> Image {
        let mut img = Image::new(w, h);
        img.d.fill(v);
        img
    }

    fn at(img: &Image, x: usize, y: usize) -> [f32; 3] {
        let i = (y * img.w + x) * 3;
        [img.d[i], img.d[i + 1], img.d[i + 2]]
    }

    /// A graduated filter from the top edge to the middle: full at the top,
    /// nothing from the middle down, and in between on the way.
    #[test]
    fn a_graduated_filter_fades_across_the_frame() {
        let mut img = flat(100, 100, 0.4);
        let f = Filter { shape: Shape::Linear { a: (0.5, 0.0), b: (0.5, 0.5) },
                         exposure: -1.0, temperature: 0.0, saturation: 0.0 };
        apply_filters(&mut img, &[f], None);
        assert!((at(&img, 50, 0)[0] - 0.2).abs() < 0.01, "top: {:?}", at(&img, 50, 0));
        assert!((at(&img, 50, 80)[0] - 0.4).abs() < 1e-5, "bottom moved: {:?}", at(&img, 50, 80));
        let mid = at(&img, 50, 25)[0];
        assert!(mid > 0.2 && mid < 0.4, "no fade between: {mid}");
    }

    /// Fractions of the framed picture, not of whatever is being rendered:
    /// shown uncropped, the filter must land where it lands when cropped.
    #[test]
    fn filters_follow_the_frame_not_the_canvas() {
        let f = Filter { shape: Shape::Radial { centre: (0.5, 0.5), radius: (0.1, 0.1),
                                                feather: 0.1, invert: false },
                         exposure: 1.0, temperature: 0.0, saturation: 0.0 };
        let mut cropped = flat(50, 50, 0.3);
        apply_filters(&mut cropped, &[f.clone()], None);
        let mut canvas = flat(100, 100, 0.3);
        apply_filters(&mut canvas, &[f], Some((0.5, 0.5, 0.5, 0.5)));
        assert_eq!(at(&cropped, 25, 25), at(&canvas, 75, 75));
        assert_eq!(at(&canvas, 25, 25), [0.3; 3], "the filter reached outside the frame");
    }

    #[test]
    fn an_inverted_radial_leaves_the_centre_alone() {
        let mut img = flat(80, 80, 0.5);
        let f = Filter { shape: Shape::Radial { centre: (0.5, 0.5), radius: (0.2, 0.2),
                                                feather: 0.3, invert: true },
                         exposure: -1.0, temperature: 0.0, saturation: 0.0 };
        apply_filters(&mut img, &[f], None);
        assert_eq!(at(&img, 40, 40), [0.5; 3]);
        assert!(at(&img, 1, 1)[0] < 0.3);
    }

    /// A dark speck on a smooth gradient disappears, and the patch takes on
    /// the brightness of where it lands rather than of where it came from.
    #[test]
    fn healing_removes_a_speck() {
        let (w, h) = (200usize, 120usize);
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.2 + 0.4 * x as f32 / w as f32;
                img.d[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[v, v * 0.9, v * 1.1]);
            }
        }
        let clean = img.clone();
        for dy in -3i64..=3 {
            for dx in -3i64..=3 {
                if dx * dx + dy * dy <= 9 {
                    let i = ((60 + dy) as usize * w + (100 + dx) as usize) * 3;
                    img.d[i..i + 3].copy_from_slice(&[0.02, 0.02, 0.02]);
                }
            }
        }
        let spot = Spot { at: (100.0 / w as f32, 60.0 / h as f32), r: 6.0 / w as f32, from: None };
        heal(&mut img, &[spot], None);
        let got = at(&img, 100, 60);
        let want = at(&clean, 100, 60);
        for c in 0..3 {
            assert!((got[c] - want[c]).abs() / want[c] < 0.05, "speck remains: {got:?} vs {want:?}");
        }
    }

    #[test]
    fn settings_read_from_the_wire() {
        let v = serde_json::json!([
            {"kind": "linear", "a": [0.5, 0.0], "b": [0.5, 0.4], "exposure": -9},
            {"kind": "radial", "centre": [0.3, 0.4], "radius": [0.1, 0.2], "invert": true,
             "saturation": 0.5},
            {"kind": "brush"}]);
        let f = filters_from_json(Some(&v));
        assert_eq!(f.len(), 2, "an unknown kind must be dropped, not guessed at");
        assert_eq!(f[0].exposure, -3.0, "exposure was not clamped");
        let s = spots_from_json(Some(&serde_json::json!([{"at": [0.2, 0.3], "r": 0.02}])));
        assert_eq!(s[0].from, None);
    }
}

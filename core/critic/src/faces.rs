//! Faces in a photograph, and whether their eyes are open and they are in
//! focus: MediaPipe's full-range face detector (BlazeFace) to find them, and
//! its face landmark model to place 468 points on each, of which the twelve
//! round the eyes say how open they are.
//!
//! Both models are Apache-2.0, baked by `tools/bake-faces.mjs`: the detector
//! from MediaPipe's face detection, the landmarks from its Face Landmarker,
//! whose eyelids follow a blink where the older mesh's do not. What is done
//! around them here follows MediaPipe's own graphs: the detector sees the
//! picture letterboxed into a square, its boxes are decoded against a grid
//! of anchors and merged by weighted non-maximum suppression, and the
//! landmark model sees a square half again as wide as the face, turned so
//! the eyes are level.

use crate::graph::Graph;

/// The detector's grid: one anchor per cell, at the cell's centre.
const GRID: usize = 48;
/// MediaPipe keeps detections above this.
const MIN_SCORE: f32 = 0.6;
/// Detections overlapping more than this are one face.
const SAME_FACE: f32 = 0.3;
/// Faces narrower than this many pixels are found but not measured.
const MIN_FACE_PX: f32 = 36.0;
/// Nor are their eyes read below this: the landmarks are guesses there.
const MIN_EYES_PX: f32 = 80.0;
/// The landmark model's own confidence that it was shown a face, below which
/// its eyes are not believed.
const MIN_PRESENCE: f32 = 0.5;
/// At most this many faces are looked into, largest first.
const MAX_FACES: usize = 8;
/// The side the sharpness of a face is measured at.
const SHARP_SIDE: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// The box, in the picture's pixels.
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub score: f32,
    /// How sure the landmark model is that it was shown a face, or `None`
    /// when the face was too small to look into.
    pub presence: Option<f32>,
    /// Each eye's height over its width, the subject's right first: about
    /// 0.25 to 0.3 open, 0.1 to 0.15 squinting, under 0.1 shut.
    pub eyes: Option<[f32; 2]>,
    /// The variance of the Laplacian over the face, at a fixed size: only
    /// meaningful against the same face in another frame.
    pub sharpness: Option<f32>,
}

/// A detection as the detector gives it: box centre and size, and six
/// keypoints (eyes, nose, mouth, ears), all in the picture's pixels.
#[derive(Clone, Debug)]
struct Detection {
    score: f32,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    keypoints: [[f32; 2]; 6],
}

/// An RGBA picture, read with bilinear sampling and clamped edges.
struct Picture<'a> {
    rgba: &'a [u8],
    w: usize,
    h: usize,
}

impl Picture<'_> {
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let x = x.clamp(0.0, self.w as f32 - 1.0);
        let y = y.clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize, c: usize| self.rgba[(yy * self.w + xx) * 4 + c] as f32;
        std::array::from_fn(|c| {
            let top = at(x0, y0, c) + (at(x1, y0, c) - at(x0, y0, c)) * fx;
            let bottom = at(x0, y1, c) + (at(x1, y1, c) - at(x0, y1, c)) * fx;
            (top + (bottom - top) * fy) / 255.0
        })
    }

    /// A square of `side` pixels, centred on `centre`, `size` pixels of the
    /// picture across, with its x axis along `axis` (a unit vector). Each
    /// output pixel averages a grid of samples as fine as the picture is.
    fn crop(&self, centre: [f32; 2], size: f32, axis: [f32; 2], side: usize) -> Vec<[f32; 3]> {
        let step = size / side as f32;
        let n = (step.ceil() as usize).clamp(1, 4);
        let perp = [-axis[1], axis[0]];
        let mut out = vec![[0.0f32; 3]; side * side];
        for v in 0..side {
            for u in 0..side {
                let mut acc = [0.0f32; 3];
                for j in 0..n {
                    for i in 0..n {
                        let lx = ((u as f32 + (i as f32 + 0.5) / n as f32) / side as f32 - 0.5) * size;
                        let ly = ((v as f32 + (j as f32 + 0.5) / n as f32) / side as f32 - 0.5) * size;
                        let px = self.sample(centre[0] + lx * axis[0] + ly * perp[0],
                                             centre[1] + lx * axis[1] + ly * perp[1]);
                        for c in 0..3 {
                            acc[c] += px[c];
                        }
                    }
                }
                let k = 1.0 / (n * n) as f32;
                out[v * side + u] = acc.map(|a| a * k);
            }
        }
        out
    }
}

fn sigmoid(v: f32) -> f32 {
    1.0 / (1.0 + (-v.clamp(-100.0, 100.0)).exp())
}

/// Every face in an RGBA picture, largest first.
pub fn find(detector: &Graph, landmarks: Option<&Graph>, rgba: &[u8], w: usize, h: usize)
            -> Vec<Face> {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return Vec::new();
    }
    let picture = Picture { rgba, w, h };
    let mut faces = Vec::new();
    for d in detect(detector, &picture) {
        let mut face = Face {
            x0: d.cx - d.w / 2.0,
            y0: d.cy - d.h / 2.0,
            x1: d.cx + d.w / 2.0,
            y1: d.cy + d.h / 2.0,
            score: d.score,
            presence: None,
            eyes: None,
            sharpness: None,
        };
        if faces.len() < MAX_FACES && d.w.max(d.h) >= MIN_FACE_PX {
            look_into(&mut face, &d, &picture, landmarks);
        }
        faces.push(face);
    }
    faces
}

fn detect(detector: &Graph, picture: &Picture) -> Vec<Detection> {
    let (side, _, _) = detector.input_size();
    // Letterboxed: the whole picture, centred, its long side the square's.
    let scale = side as f32 / picture.w.max(picture.h) as f32;
    let (ox, oy) = ((side as f32 - picture.w as f32 * scale) / 2.0,
                    (side as f32 - picture.h as f32 * scale) / 2.0);
    let centre = [picture.w as f32 / 2.0, picture.h as f32 / 2.0];
    let pixels = picture.crop(centre, side as f32 / scale, [1.0, 0.0], side);
    let mut input = Vec::with_capacity(side * side * 3);
    for (i, px) in pixels.iter().enumerate() {
        let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
        let inside = x >= ox && x <= side as f32 - ox && y >= oy && y <= side as f32 - oy;
        for c in px {
            // -1..1, the border black.
            input.push(if inside { c * 2.0 - 1.0 } else { -1.0 });
        }
    }
    let out = detector.run(input);
    let (boxes, scores) = (&out[0], &out[1]);
    let to_picture = |u: f32, v: f32| [(u * side as f32 - ox) / scale, (v * side as f32 - oy) / scale];

    let mut found = Vec::new();
    for (k, raw) in scores.iter().enumerate().take(GRID * GRID) {
        let score = sigmoid(*raw);
        if score < MIN_SCORE {
            continue;
        }
        let r = &boxes[k * 16..k * 16 + 16];
        let (ax, ay) = (((k % GRID) as f32 + 0.5) / GRID as f32, ((k / GRID) as f32 + 0.5) / GRID as f32);
        let s = side as f32;
        let c = to_picture(r[0] / s + ax, r[1] / s + ay);
        let keypoints = std::array::from_fn(|j| to_picture(r[4 + 2 * j] / s + ax, r[5 + 2 * j] / s + ay));
        found.push(Detection { score, cx: c[0], cy: c[1], w: r[2] / scale, h: r[3] / scale,
                               keypoints });
    }
    let mut merged = weighted_nms(found);
    merged.sort_by(|a, b| (b.w * b.h).partial_cmp(&(a.w * a.h)).unwrap());
    merged
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let (ax0, ay0, ax1, ay1) = (a.cx - a.w / 2.0, a.cy - a.h / 2.0, a.cx + a.w / 2.0, a.cy + a.h / 2.0);
    let (bx0, by0, bx1, by1) = (b.cx - b.w / 2.0, b.cy - b.h / 2.0, b.cx + b.w / 2.0, b.cy + b.h / 2.0);
    let iw = (ax1.min(bx1) - ax0.max(bx0)).max(0.0);
    let ih = (ay1.min(by1) - ay0.max(by0)).max(0.0);
    let inter = iw * ih;
    let union = a.w * a.h + b.w * b.h - inter;
    if union > 0.0 { inter / union } else { 0.0 }
}

/// MediaPipe's weighted suppression: every detection overlapping the best
/// one left is folded into it, weighted by score, and the best one's score kept.
fn weighted_nms(mut found: Vec<Detection>) -> Vec<Detection> {
    found.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut out = Vec::new();
    while !found.is_empty() {
        let best = found[0].clone();
        let (same, rest): (Vec<Detection>, Vec<Detection>) =
            found.into_iter().partition(|d| iou(&best, d) > SAME_FACE);
        let total: f32 = same.iter().map(|d| d.score).sum();
        let mean = |f: &dyn Fn(&Detection) -> f32| same.iter().map(|d| f(d) * d.score).sum::<f32>() / total;
        out.push(Detection {
            score: best.score,
            cx: mean(&|d| d.cx),
            cy: mean(&|d| d.cy),
            w: mean(&|d| d.w),
            h: mean(&|d| d.h),
            keypoints: std::array::from_fn(|k| [mean(&|d| d.keypoints[k][0]), mean(&|d| d.keypoints[k][1])]),
        });
        found = rest;
    }
    out
}

/// Place the landmarks on one face, read its eyes, and measure how sharp it is.
fn look_into(face: &mut Face, d: &Detection, picture: &Picture, landmarks: Option<&Graph>) {
    // Turned so the eyes are level: the crop's x axis runs from the right eye
    // to the left one.
    let (right, left) = (d.keypoints[0], d.keypoints[1]);
    let (ex, ey) = (left[0] - right[0], left[1] - right[1]);
    let len = (ex * ex + ey * ey).sqrt();
    let axis = if len > 1e-3 { [ex / len, ey / len] } else { [1.0, 0.0] };
    let centre = [d.cx, d.cy];
    let size = d.w.max(d.h);

    face.sharpness = Some(sharpness(&picture.crop(centre, size, axis, SHARP_SIDE), SHARP_SIDE));

    let Some(model) = landmarks else { return };
    if size < MIN_EYES_PX {
        return;
    }
    let (side, _, _) = model.input_size();
    let roi = size * 1.5;
    let pixels = picture.crop(centre, roi, axis, side);
    let input: Vec<f32> = pixels.iter().flatten().copied().collect();
    let out = model.run(input);
    let (points, flag) = (&out[0], &out[1]);
    face.presence = flag.first().map(|v| sigmoid(*v));
    if points.len() < 468 * 3 || face.presence.is_some_and(|p| p < MIN_PRESENCE) {
        return;
    }
    let p = |i: usize| [points[i * 3], points[i * 3 + 1]];
    face.eyes = Some([openness(p(33), p(133), [p(160), p(158)], [p(144), p(153)]),
                      openness(p(362), p(263), [p(385), p(387)], [p(380), p(373)])]);
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// How open an eye is: the gap between its lids, at two places, over its width.
fn openness(corner: [f32; 2], other: [f32; 2], upper: [[f32; 2]; 2], lower: [[f32; 2]; 2]) -> f32 {
    let width = distance(corner, other).max(1e-3);
    (distance(upper[0], lower[0]) + distance(upper[1], lower[1])) / (2.0 * width)
}

/// The variance of the Laplacian of the grey, the same measure the gallery
/// uses for a whole photograph (with its Rec.709 weights), here of one face at
/// a fixed size.
fn sharpness(pixels: &[[f32; 3]], side: usize) -> f32 {
    let grey: Vec<f32> = pixels.iter().map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).collect();
    let mut sum = 0.0f64;
    let mut sum2 = 0.0f64;
    let mut n = 0usize;
    for y in 1..side - 1 {
        for x in 1..side - 1 {
            let i = y * side + x;
            let lap = 4.0 * grey[i] - grey[i - 1] - grey[i + 1] - grey[i - side] - grey[i + side];
            let v = (lap * 255.0) as f64;
            sum += v;
            sum2 += v * v;
            n += 1;
        }
    }
    let mean = sum / n as f64;
    (sum2 / n as f64 - mean * mean) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_eye_is_taller_than_a_shut_one() {
        let open = openness([0.0, 0.0], [10.0, 0.0], [[3.0, -1.5], [7.0, -1.5]], [[3.0, 1.5], [7.0, 1.5]]);
        let shut = openness([0.0, 0.0], [10.0, 0.0], [[3.0, -0.3], [7.0, -0.3]], [[3.0, 0.3], [7.0, 0.3]]);
        assert!((open - 0.3).abs() < 1e-6 && (shut - 0.06).abs() < 1e-6);
    }

    #[test]
    fn overlapping_detections_are_one_face() {
        let d = |cx: f32, score: f32| Detection { score, cx, cy: 50.0, w: 20.0, h: 20.0,
                                                 keypoints: [[cx, 50.0]; 6] };
        let merged = weighted_nms(vec![d(50.0, 0.9), d(52.0, 0.6), d(150.0, 0.7)]);
        assert_eq!(merged.len(), 2);
        assert!((merged[0].cx - (50.0 * 0.9 + 52.0 * 0.6) / 1.5).abs() < 1e-4);
        assert_eq!(merged[0].score, 0.9);
    }

    #[test]
    fn detail_is_sharper_than_blur() {
        let side = 32;
        let checks: Vec<[f32; 3]> = (0..side * side)
            .map(|i| if (i % side + i / side) % 2 == 0 { [1.0; 3] } else { [0.0; 3] }).collect();
        let flat = vec![[0.5f32; 3]; side * side];
        assert!(sharpness(&checks, side) > 1000.0 * sharpness(&flat, side).max(1e-3));
    }

    #[test]
    fn a_crop_turned_with_the_eyes_reads_the_right_pixels() {
        // A picture whose red is its x and green its y.
        let (w, h) = (64usize, 64usize);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                rgba[(y * w + x) * 4] = (x * 4) as u8;
                rgba[(y * w + x) * 4 + 1] = (y * 4) as u8;
            }
        }
        let pic = Picture { rgba: &rgba, w, h };
        // Turned a quarter: the crop's x axis runs down the picture.
        let out = pic.crop([32.0, 32.0], 16.0, [0.0, 1.0], 16);
        let first = out[8];
        let last = out[8 + 7];
        assert!(last[1] > first[1], "along the crop's x, the picture's y grows");
        assert!((last[0] - first[0]).abs() < 0.02, "and its x stays put");
    }
}

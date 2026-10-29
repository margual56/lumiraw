//! Where the sky is, and where the subject is, for the local filters that
//! follow them rather than a line or an ellipse.
//!
//! Both are found once, on a small copy of the framed picture, and kept as a
//! coarse plane over the frame (see `Masks`). Each render samples that plane
//! and lets the picture itself sharpen the edge (`refine`), so the same coarse
//! mask serves a preview, a piece at 100 % and the export.

use critic::graph::Graph;
use kit::{Image, Plane};

/// The long edge the sky is looked for at.
pub const SKY_EDGE: usize = 384;

/// Which mask a filter follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Sky,
    Subject,
}

impl Kind {
    pub fn from_code(code: &str) -> Option<Kind> {
        match code {
            "sky" => Some(Kind::Sky),
            "subject" => Some(Kind::Subject),
            _ => None,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Kind::Sky => "sky",
            Kind::Subject => "subject",
        }
    }
}

/// The masks found for one framing: each 0..1 over the whole framed picture,
/// at whatever size it was found at.
#[derive(Clone, Debug, Default)]
pub struct Masks {
    pub sky: Option<Plane>,
    pub subject: Option<Plane>,
}

impl Masks {
    pub fn get(&self, kind: Kind) -> Option<&Plane> {
        match kind {
            Kind::Sky => self.sky.as_ref(),
            Kind::Subject => self.subject.as_ref(),
        }
    }
}

/// Oklab of a display-linear picture, as three planes.
fn lab(img: &Image) -> (Plane, Plane, Plane) {
    let (mut l, mut a, mut b) = (Plane::new(img.w, img.h), Plane::new(img.w, img.h),
                                 Plane::new(img.w, img.h));
    for (i, px) in img.px().iter().enumerate() {
        let (ll, aa, bb) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
        l.d[i] = ll;
        a.d[i] = aa;
        b.d[i] = bb;
    }
    (l, a, b)
}

/// Where the sky is in a display-linear picture, without a network: the
/// smooth, light part of the frame that reaches its top edge, and whatever
/// else has that same colour above the horizon (sky between branches).
pub fn sky(img: &Image) -> Plane {
    let small = kit::thumbnail(img, SKY_EDGE);
    let (w, h) = (small.w, small.h);
    let (l, a, b) = lab(&small);
    // Texture: the local spread of lightness.
    let mean = kit::box_blur(&l, 2);
    let mut sq = l.clone();
    sq.d.iter_mut().for_each(|v| *v *= *v);
    let mean_sq = kit::box_blur(&sq, 2);
    let texture: Vec<f32> = (0..w * h).map(|i| (mean_sq.d[i] - mean.d[i] * mean.d[i]).max(0.0).sqrt())
        .collect();
    let median = kit::percentile(&l.d, 50.0);
    let candidate: Vec<bool> = (0..w * h).map(|i| {
        let foliage = a.d[i] < -0.03 && b.d[i] > 0.02;
        texture[i] < 0.025 && l.d[i] > (median * 0.9).max(0.35) && !foliage
            && a.d[i].hypot(b.d[i]) < 0.16
    }).collect();

    // Grown from the top edge through candidates, a step at a time, so a
    // smooth wall below a sky it touches is not taken for more of it. Each
    // patch so grown has to be a real share of the frame: a white sign along
    // the top is smooth and light too.
    let mut sky = vec![false; w * h];
    let mut seen = vec![false; w * h];
    for seed in 0..w {
        if seen[seed] || !candidate[seed] {
            continue;
        }
        seen[seed] = true;
        let mut patch = vec![seed];
        let mut stack = vec![seed];
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !seen[j] && candidate[j] && (l.d[j] - l.d[i]).abs() < 0.03 {
                    seen[j] = true;
                    stack.push(j);
                    patch.push(j);
                }
            };
            if x > 0 { visit(i - 1); }
            if x + 1 < w { visit(i + 1); }
            if y > 0 { visit(i - w); }
            if y + 1 < h { visit(i + w); }
        }
        if patch.len() * 50 >= w * h {
            for i in patch {
                sky[i] = true;
            }
        }
    }
    let found = sky.iter().filter(|s| **s).count();
    let mut out = Plane::new(w, h);
    if found == 0 {
        return out;
    }

    // Sky seen through gaps: the same colour, no lower than the sky reaches.
    let (mut ml, mut ma, mut mb) = (0f64, 0f64, 0f64);
    let mut lowest = 0;
    for i in (0..w * h).filter(|&i| sky[i]) {
        ml += l.d[i] as f64;
        ma += a.d[i] as f64;
        mb += b.d[i] as f64;
        lowest = lowest.max(i / w);
    }
    let n = found as f64;
    let (ml, ma, mb) = ((ml / n) as f32, (ma / n) as f32, (mb / n) as f32);
    for i in 0..w * h {
        let close = ((l.d[i] - ml).powi(2) + (a.d[i] - ma).powi(2) + (b.d[i] - mb).powi(2)).sqrt();
        out.d[i] = if sky[i] || (i / w <= lowest && candidate[i] && close < 0.06) { 1.0 } else { 0.0 };
    }
    kit::gaussian_blur(&out, 1.0)
}

/// How sure the segmenter is that each pixel is not background, over the
/// whole picture: the picture is squashed to the network's square, as the
/// model was trained, and the answer stretched back to the picture's shape.
pub fn subject(img: &Image, segmenter: &Graph) -> Plane {
    let (side, _, _) = segmenter.input_size();
    let small = kit::resize_rgb(img, side, side);
    let input: Vec<f32> = small.px().iter()
        .flat_map(|px| px.map(|v| kit::srgb_encode_scalar(v.clamp(0.0, 1.0)) * 2.0 - 1.0))
        .collect();
    let out = segmenter.run(input);
    let logits = &out[0];
    let classes = logits.len() / (side * side);
    let mut plane = Plane::new(side, side);
    for (i, p) in plane.d.iter_mut().enumerate() {
        let row = &logits[i * classes..(i + 1) * classes];
        let top = row.iter().cloned().fold(f32::MIN, f32::max);
        let sum: f32 = row.iter().map(|v| (v - top).exp()).sum();
        *p = 1.0 - (row[0] - top).exp() / sum;
    }
    let scale = side as f32 / img.w.max(img.h) as f32;
    kit::resize_plane(&plane, ((img.w as f32 * scale).round() as usize).max(1),
                      ((img.h as f32 * scale).round() as usize).max(1))
}

/// A coarse mask laid over an image, `frame` placing the framed picture in it
/// (as `local::apply_filters` has it), with its edge pulled onto the edges in
/// the image: a guided filter, the image's lightness the guide.
pub fn refine(mask: &Plane, img: &Image, frame: (f32, f32, f32, f32)) -> Plane {
    let (fx, fy, fw, fh) = frame;
    let mut out = Plane::new(img.w, img.h);
    for y in 0..img.h {
        let v = (y as f32 + 0.5 - fy) / fh;
        for x in 0..img.w {
            let u = (x as f32 + 0.5 - fx) / fw;
            out.d[y * img.w + x] = kit::sample_bilinear(mask, u * mask.w as f32 - 0.5,
                                                        v * mask.h as f32 - 0.5);
        }
    }
    let mut guide = kit::luminance(img);
    guide.d.iter_mut().for_each(|v| *v = v.max(0.0).sqrt());
    // About one mask pixel across, so the edge can move as far as the mask
    // could be wrong and no further.
    let radius = ((fw.max(fh) / mask.w.max(mask.h) as f32) * 1.5).round().max(2.0) as usize;
    let mut refined = kit::guided_filter(&guide, &out, radius, 1e-3, (radius / 4).max(1));
    refined.d.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));
    refined
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blue sky over a textured, darker ground.
    fn landscape(w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for (i, px) in img.px_mut().iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            *px = if y < h / 2 {
                let t = y as f32 / h as f32;
                [0.25 + 0.2 * t, 0.45 + 0.15 * t, 0.85]
            } else {
                let n = (((x * 7919 + y * 104729) % 97) as f32 / 97.0) * 0.12;
                [0.08 + n, 0.12 + n, 0.04 + n * 0.5]
            };
        }
        img
    }

    #[test]
    fn the_sky_is_the_top_and_not_the_ground() {
        let img = landscape(300, 200);
        let m = sky(&img);
        let at = |fx: f32, fy: f32| kit::sample_bilinear(&m, fx * m.w as f32, fy * m.h as f32);
        assert!(at(0.5, 0.15) > 0.95, "sky missed: {}", at(0.5, 0.15));
        assert!(at(0.5, 0.85) < 0.05, "ground taken for sky: {}", at(0.5, 0.85));
    }

    #[test]
    fn a_frame_without_sky_has_none() {
        let mut img = landscape(300, 200);
        // Ground all the way up.
        for y in 0..100 {
            for x in 0..300 {
                img.px_mut()[y * 300 + x] = img.px()[(y + 100) * 300 + x];
            }
        }
        let m = sky(&img);
        assert!(m.d.iter().all(|v| *v < 0.05));
    }

    #[test]
    fn the_baked_segmenter_reads() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),
                                          "/../web/src/lib/wasm/critic/segmenter.bin")).unwrap();
        let net = Graph::parse(&bytes).unwrap();
        assert_eq!(net.input_size(), (257, 257, 3));
    }

    #[test]
    fn refining_follows_the_picture() {
        let img = landscape(120, 80);
        // A mask whose edge is a few pixels off the horizon.
        let mut coarse = Plane::new(12, 8);
        for y in 0..8 {
            for x in 0..12 {
                coarse.d[y * 12 + x] = if y < 5 { 1.0 } else { 0.0 };
            }
        }
        let fine = refine(&coarse, &img, (0.0, 0.0, 120.0, 80.0));
        // Just below the horizon, inside the coarse mask, the ground pulls the
        // mask down.
        let row = |y: usize| (0..120).map(|x| fine.d[y * 120 + x]).sum::<f32>() / 120.0;
        assert!(row(20) > 0.9, "the sky went: {}", row(20));
        assert!(row(46) < 0.5, "the ground under the mask stayed in it: {}", row(46));
    }
}

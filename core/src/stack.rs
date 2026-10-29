//! Many frames of one scene folded into one: averaged, for less noise or a
//! long exposure made of short ones; the median, so whatever passed through
//! is gone; the brightest of each pixel, for star trails; or the sharpest
//! part of each, for a focus stack.
//!
//! Frames come in one at a time and are let go once folded in, so a stack is
//! not limited by how many frames fit in memory at once. The median is the
//! exception, since it cannot be had without every reading: it keeps a
//! half-float copy of each frame, up to a budget.

use crate::merge::{self, Finding, Frame};
use kit::{Image, Plane};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Mean,
    Median,
    Lighten,
    Focus,
}

impl Mode {
    pub fn from_code(code: &str) -> Option<Mode> {
        Some(match code {
            "mean" => Mode::Mean,
            "median" => Mode::Median,
            "lighten" => Mode::Lighten,
            "focus" => Mode::Focus,
            _ => return None,
        })
    }

    pub fn code(self) -> &'static str {
        match self {
            Mode::Mean => "mean",
            Mode::Median => "median",
            Mode::Lighten => "lighten",
            Mode::Focus => "focus",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub mode: Mode,
    /// Line each frame up with the first before folding it in. Off for star
    /// trails on a tripod, where lining up would chase the stars.
    pub align: bool,
    /// How many bytes the median may keep.
    pub budget: usize,
}

impl Options {
    pub fn new(mode: Mode, align: bool) -> Options {
        Options { mode, align, budget: MEDIAN_BUDGET }
    }
}

/// What the median may keep: ten frames of 24 megapixels.
const MEDIAN_BUDGET: usize = 1_500_000_000;

/// How far a focus stack's frames are searched for a change of magnification,
/// either way. Focus breathing is a few per cent on most lenses.
const BREATHING: f32 = 0.06;
/// Below this the magnification is left alone: a fraction of a pixel at the
/// frame's edge.
const SAME_SCALE: f32 = 0.0003;

/// How strongly the sharper frame wins where two disagree. Twice the detail is
/// 64 times the weight: as good as a choice, without its seams.
const FOCUS_POWER: i32 = 6;

/// What happened to one frame on the way in.
#[derive(Clone, Debug, PartialEq)]
pub struct Added {
    pub index: usize,
    /// Why it was left out, if it was.
    pub left_out: Option<&'static str>,
    pub shift: (i32, i32),
    pub scale: f32,
    /// Where it sits in stops from the first frame, as it was scaled.
    pub stops: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Notes {
    /// How many frames went into the result.
    pub used: usize,
    pub added: Vec<Added>,
    pub findings: Vec<Finding>,
}

/// The first frame, which every other is lined up with and scaled to.
struct Reference {
    w: usize,
    h: usize,
    frame: Frame,
    bitmaps: merge::Bitmaps,
    thumb: Image,
    grey: Option<Plane>,
}

enum Fold {
    Mean { sum: Vec<f32>, count: Vec<f32> },
    Lighten { max: Vec<f32> },
    /// Each frame as half floats, with NaN where it did not reach.
    Median { frames: Vec<Vec<u16>> },
    Focus { pyramid: Pyramid },
}

/// A stack being gathered.
pub struct Stacker {
    options: Options,
    reference: Option<Reference>,
    fold: Option<Fold>,
    notes: Notes,
}

impl Stacker {
    pub fn new(options: Options) -> Stacker {
        Stacker { options, reference: None, fold: None, notes: Notes::default() }
    }

    pub fn mode(&self) -> Mode {
        self.options.mode
    }

    /// Fold one frame in. The first is the reference, so it is the one whose
    /// framing, exposure and metadata the result keeps.
    pub fn add(&mut self, frame: Frame) -> &Added {
        let index = self.notes.added.len();
        let added = self.place(frame, index);
        self.notes.added.push(added);
        self.notes.added.last().unwrap()
    }

    /// Line a frame up and fold it in, or say why it was left out.
    fn place(&mut self, mut frame: Frame, index: usize) -> Added {
        let mut added = Added { index, left_out: None, shift: (0, 0), scale: 1.0, stops: 0.0 };
        let mut image = std::mem::replace(&mut frame.linear, Image::new(0, 0));
        let Some(reference) = &self.reference else {
            self.reference = Some(Reference {
                w: image.w,
                h: image.h,
                bitmaps: merge::median_bitmaps(&image),
                thumb: kit::thumbnail(&image, merge::MEASURE_EDGE),
                // Only a focus stack measures magnification against it.
                grey: (self.options.align && self.options.mode == Mode::Focus)
                    .then(|| small_grey(&image)),
                frame,
            });
            self.fold_in(image, None);
            return added;
        };

        if image.w != reference.w || image.h != reference.h {
            added.left_out = Some("stack_size");
            return added;
        }
        if let Some(Fold::Median { frames }) = &self.fold {
            if (frames.len() + 1) * image.w * image.h * 6 > self.options.budget {
                added.left_out = Some("stack_memory");
                return added;
            }
        }

        // The same exposure, by the metadata's account, unless the pixels say
        // otherwise as plainly as the merge would have them.
        added.stops = frame.stops_from(&reference.frame);
        if let Some((measured, spread)) = merge::measured_stops(&reference.thumb, &image) {
            if merge::overrules(added.stops, measured, spread) {
                added.stops = measured;
            }
        }

        let mut covered: Option<Vec<bool>> = None;
        if self.options.align {
            if let Some(grey) = &reference.grey {
                let scale = fit_scale(grey, &small_grey(&image));
                if (scale - 1.0).abs() > SAME_SCALE {
                    let (scaled, reach) = warp(&image, scale, (0, 0));
                    (image, covered, added.scale) = (scaled, Some(reach), scale);
                }
            }
            match merge::align(&reference.bitmaps, &merge::median_bitmaps(&image)) {
                Some(shift) => added.shift = shift,
                None => {
                    added.left_out = Some("no_correspondence");
                    return added;
                }
            }
            if added.shift != (0, 0) {
                // A whole-pixel shift is an exact copy, and what the scaling
                // left uncovered moves with it.
                let (shifted, mut reach) = warp(&image, 1.0, added.shift);
                if let Some(before) = &covered {
                    let (w, (dx, dy)) = (image.w, added.shift);
                    for (i, r) in reach.iter_mut().enumerate() {
                        if *r {
                            let (x, y) = ((i % w) as i32 + dx, (i / w) as i32 + dy);
                            *r = before[y as usize * w + x as usize];
                        }
                    }
                }
                (image, covered) = (shifted, Some(reach));
            }
        }

        let gain = (-added.stops).exp2();
        if (gain - 1.0).abs() > 1e-4 {
            image.scale(gain);
        }
        self.fold_in(image, covered.as_deref());
        added
    }

    fn fold_in(&mut self, image: Image, covered: Option<&[bool]>) {
        let reaches = |i: usize| covered.is_none_or(|c| c[i]);
        let n = image.w * image.h;
        match self.options.mode {
            Mode::Mean => {
                let fold = self.fold.get_or_insert_with(|| Fold::Mean {
                    sum: vec![0.0; n * 3],
                    count: vec![0.0; n],
                });
                let Fold::Mean { sum, count } = fold else { unreachable!() };
                for i in 0..n {
                    if reaches(i) {
                        count[i] += 1.0;
                        for c in 0..3 {
                            sum[i * 3 + c] += image.d[i * 3 + c];
                        }
                    }
                }
            }
            Mode::Lighten => match &mut self.fold {
                None => self.fold = Some(Fold::Lighten { max: image.d }),
                Some(Fold::Lighten { max }) => {
                    for i in 0..n {
                        if reaches(i) {
                            for c in 0..3 {
                                let v = image.d[i * 3 + c];
                                if v > max[i * 3 + c] {
                                    max[i * 3 + c] = v;
                                }
                            }
                        }
                    }
                }
                _ => unreachable!(),
            },
            Mode::Median => {
                let half: Vec<u16> = image.d.iter().enumerate()
                    .map(|(k, v)| if reaches(k / 3) { to_half(*v) } else { HALF_NAN })
                    .collect();
                drop(image);
                match &mut self.fold {
                    None => self.fold = Some(Fold::Median { frames: vec![half] }),
                    Some(Fold::Median { frames }) => frames.push(half),
                    _ => unreachable!(),
                }
            }
            Mode::Focus => {
                let fold = self.fold.get_or_insert_with(|| Fold::Focus {
                    pyramid: Pyramid::new(image.w, image.h),
                });
                let Fold::Focus { pyramid } = fold else { unreachable!() };
                pyramid.add(image, covered);
            }
        }
    }

    /// The stacked frame, scene-linear, and what was done to get it.
    pub fn finish(self) -> Result<(Image, Frame, Notes), String> {
        let Stacker { reference, fold, mut notes, .. } = self;
        let (Some(reference), Some(fold)) = (reference, fold) else {
            return Err("a stack needs at least two frames".into());
        };
        notes.used = notes.added.iter().filter(|a| a.left_out.is_none()).count();
        if notes.used < 2 {
            return Err("a stack needs at least two frames that line up".into());
        }
        let (w, h) = (reference.w, reference.h);
        let n = w * h;
        let mut out = Image::new(w, h);
        match fold {
            Fold::Mean { sum, count } => {
                for i in 0..n {
                    let k = 1.0 / count[i].max(1.0);
                    for c in 0..3 {
                        out.d[i * 3 + c] = sum[i * 3 + c] * k;
                    }
                }
            }
            Fold::Lighten { max } => out.d = max,
            Fold::Median { frames } => {
                let mut values = Vec::with_capacity(frames.len());
                for (k, v) in out.d.iter_mut().enumerate() {
                    values.clear();
                    values.extend(frames.iter().map(|f| from_half(f[k])).filter(|v| !v.is_nan()));
                    values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
                    *v = kit::percentile_sorted(&values, 50.0);
                }
            }
            Fold::Focus { pyramid } => out = pyramid.collapse(),
        }
        for code in ["no_correspondence", "stack_size", "stack_memory"] {
            let frames: Vec<usize> = notes.added.iter()
                .filter(|a| a.left_out == Some(code)).map(|a| a.index).collect();
            if !frames.is_empty() {
                let value = if code == "stack_memory" { notes.used as f32 } else { 0.0 };
                notes.findings.push(Finding { code, frames, value });
            }
        }
        Ok((out, reference.frame, notes))
    }
}

// -------------------------------------------------------------------------
// lining up
// -------------------------------------------------------------------------

/// The long edge the magnification is measured at.
const GREY_EDGE: usize = 512;

/// A small grey copy, on a perceptual-ish scale, for measuring magnification.
fn small_grey(img: &Image) -> Plane {
    crate::analyze::sqrt_luminance(&kit::thumbnail(img, GREY_EDGE))
}

/// The frame as it would be had it been taken at the reference's
/// magnification and position: read at `centre + scale·(x + shift − centre)`.
/// Pixels it does not reach are filled from its edge and marked.
pub fn warp(img: &Image, scale: f32, (dx, dy): (i32, i32)) -> (Image, Vec<bool>) {
    let (w, h) = (img.w, img.h);
    let (cx, cy) = ((w as f32 - 1.0) * 0.5, (h as f32 - 1.0) * 0.5);
    let mut out = Image::new(w, h);
    let mut covered = vec![true; w * h];
    let px = out.px_mut();
    for y in 0..h {
        let sy = cy + scale * (y as f32 + dy as f32 - cy);
        let inside_y = sy >= 0.0 && sy <= h as f32 - 1.0;
        for x in 0..w {
            let sx = cx + scale * (x as f32 + dx as f32 - cx);
            let i = y * w + x;
            covered[i] = inside_y && sx >= 0.0 && sx <= w as f32 - 1.0;
            px[i] = if scale == 1.0 {
                let (ix, iy) = ((sx as i32).clamp(0, w as i32 - 1) as usize,
                                (sy as i32).clamp(0, h as i32 - 1) as usize);
                img.px()[iy * w + ix]
            } else {
                kit::sample_bilinear_rgb(img, sx, sy)
            };
        }
    }
    (out, covered)
}

/// How much larger `other` shows the scene than `anchor`, both small grey
/// copies: what focus breathing does to a focus stack. Found coarse to fine,
/// each scale tried with a small search over position, by the mean difference
/// once both are brought to the same mean and spread.
pub fn fit_scale(anchor: &Plane, other: &Plane) -> f32 {
    if anchor.w != other.w || anchor.h != other.h {
        return 1.0;
    }
    let coarse = |p: &Plane| standardised(&kit::resize_plane(p, p.w / 4, p.h / 4));
    let (a4, o4) = (coarse(anchor), coarse(other));
    // Blurred a little, so that where one frame is in focus and the other is
    // not, what is compared is where things are rather than how crisp.
    let soft = |p: &Plane| standardised(&kit::gaussian_blur(p, 2.0));
    let (a1, o1) = (soft(anchor), soft(other));

    let mut best = (1.0f32, 0.0f32, 0.0f32);
    let mut best_error = f32::MAX;
    let steps = (BREATHING / 0.01).round() as i32;
    for k in -steps..=steps {
        let s = 1.0 + k as f32 * 0.01;
        for dy in -3..=3 {
            for dx in -3..=3 {
                let e = difference(&a4, &o4, s, dx as f32, dy as f32);
                if e < best_error {
                    best_error = e;
                    best = (s, dx as f32, dy as f32);
                }
            }
        }
    }
    let (s0, dx0, dy0) = (best.0, best.1 * 4.0, best.2 * 4.0);
    best_error = f32::MAX;
    for k in -6..=6 {
        let s = s0 + k as f32 * 0.002;
        for dy in -3..=3 {
            for dx in -3..=3 {
                let (fx, fy) = (dx0 + dx as f32, dy0 + dy as f32);
                let e = difference(&a1, &o1, s, fx, fy);
                if e < best_error {
                    best_error = e;
                    best = (s, fx, fy);
                }
            }
        }
    }
    // Refined once more, between the steps tried, by a parabola through the
    // best and its neighbours.
    let (s, fx, fy) = best;
    let (lo, hi) = (difference(&a1, &o1, s - 0.002, fx, fy), difference(&a1, &o1, s + 0.002, fx, fy));
    let bend = lo + hi - 2.0 * best_error;
    let nudge = if bend > 1e-9 { (0.5 * (lo - hi) / bend).clamp(-1.0, 1.0) * 0.002 } else { 0.0 };
    s + nudge
}

/// Zero mean, unit spread.
fn standardised(p: &Plane) -> Plane {
    let mean = kit::mean(&p.d);
    let spread = kit::std_dev(&p.d).max(1e-6);
    Plane { w: p.w, h: p.h, d: p.d.iter().map(|v| (v - mean) / spread).collect() }
}

/// The mean absolute difference between `a` and `b` read at
/// `centre + s·(x − centre) + shift`, over the middle of the frame, which
/// every candidate reaches.
fn difference(a: &Plane, b: &Plane, s: f32, dx: f32, dy: f32) -> f32 {
    let (w, h) = (a.w, a.h);
    let (cx, cy) = ((w as f32 - 1.0) * 0.5, (h as f32 - 1.0) * 0.5);
    let (mx, my) = (w / 8 + 2, h / 8 + 2);
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for y in my..h.saturating_sub(my) {
        let sy = cy + s * (y as f32 - cy) + dy;
        for x in mx..w.saturating_sub(mx) {
            let sx = cx + s * (x as f32 - cx) + dx;
            sum += (a.d[y * w + x] - kit::sample_bilinear(b, sx, sy)).abs();
            count += 1;
        }
    }
    if count == 0 { f32::MAX } else { sum / count as f32 }
}

// -------------------------------------------------------------------------
// focus
// -------------------------------------------------------------------------

/// A focus stack in progress: for every level of a Laplacian pyramid, the
/// frames' detail weighted by how sharp each frame is there, and the sum of
/// those weights. The weights are blurred down the pyramid along with the
/// detail, so where the choice of frame changes, it changes gradually at the
/// coarse scales and sharply at the fine ones, and leaves no seam.
///
/// The work is done on the square root of the light, which is closer to how
/// it looks: blended linearly, a dark edge next to a bright one rings.
struct Pyramid {
    sizes: Vec<(usize, usize)>,
    /// Per level, three channels interleaved.
    detail: Vec<Vec<f32>>,
    weight: Vec<Vec<f32>>,
}

impl Pyramid {
    fn new(w: usize, h: usize) -> Pyramid {
        let mut sizes = vec![(w, h)];
        while sizes.len() < 12 {
            let (lw, lh) = *sizes.last().unwrap();
            if lw.min(lh) <= 16 {
                break;
            }
            sizes.push(((lw + 1) / 2, (lh + 1) / 2));
        }
        let detail = sizes.iter().map(|(w, h)| vec![0.0; w * h * 3]).collect();
        let weight = sizes.iter().map(|(w, h)| vec![0.0; w * h]).collect();
        Pyramid { sizes, detail, weight }
    }

    fn add(&mut self, image: Image, covered: Option<&[bool]>) {
        let (w, h) = (image.w, image.h);
        let root = |v: f32| v.max(0.0).sqrt();

        // How much detail there is around each pixel, from the grey.
        let mut grey = Plane::new(w, h);
        for (g, px) in grey.d.iter_mut().zip(image.px()) {
            *g = kit::luminance_px(&px.map(root));
        }
        let mut contrast = Plane::new(w, h);
        for y in 0..h {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
            for x in 0..w {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(w - 1));
                let centre = grey.d[y * w + x];
                let lap = 4.0 * centre - grey.d[y * w + left] - grey.d[y * w + right]
                    - grey.d[up * w + x] - grey.d[down * w + x];
                contrast.d[y * w + x] = lap.abs();
            }
        }
        drop(grey);
        let sigma = (w.max(h) as f32 / 1500.0).max(1.5);
        let mut weight = kit::gaussian_blur(&contrast, sigma);
        drop(contrast);
        for (i, v) in weight.d.iter_mut().enumerate() {
            *v = if covered.is_none_or(|c| c[i]) { (*v + 1e-4).powi(FOCUS_POWER) } else { 0.0 };
        }

        // The weights' Gaussian pyramid, accumulated level by level.
        let levels = self.sizes.len();
        let mut weights = vec![weight];
        for l in 1..levels {
            let next = reduce(&weights[l - 1]);
            weights.push(next);
        }
        for (l, g) in weights.iter().enumerate() {
            for (acc, v) in self.weight[l].iter_mut().zip(&g.d) {
                *acc += *v;
            }
        }

        // Each channel's Laplacian pyramid, weighted in, one channel at a time.
        for c in 0..3 {
            let mut level = image.plane(c);
            for v in level.d.iter_mut() {
                *v = root(*v);
            }
            for l in 0..levels {
                let (detail, next) = if l + 1 < levels {
                    let next = reduce(&level);
                    let up = expand(&next, level.w, level.h);
                    let mut d = level;
                    for (v, u) in d.d.iter_mut().zip(&up.d) {
                        *v -= *u;
                    }
                    (d, Some(next))
                } else {
                    (level, None)
                };
                let acc = &mut self.detail[l];
                for (i, (v, wt)) in detail.d.iter().zip(&weights[l].d).enumerate() {
                    acc[i * 3 + c] += *v * *wt;
                }
                match next {
                    Some(next) => level = next,
                    None => break,
                }
            }
        }
    }

    fn collapse(self) -> Image {
        let levels = self.sizes.len();
        let mut planes: Vec<Plane> = Vec::new();
        for c in 0..3 {
            let mut level: Option<Plane> = None;
            for l in (0..levels).rev() {
                let (w, h) = self.sizes[l];
                let mut p = match level {
                    Some(coarser) => expand(&coarser, w, h),
                    None => Plane::new(w, h),
                };
                for (i, v) in p.d.iter_mut().enumerate() {
                    let wt = self.weight[l][i];
                    if wt > 0.0 {
                        *v += self.detail[l][i * 3 + c] / wt;
                    }
                }
                level = Some(p);
            }
            let mut p = level.unwrap();
            for v in p.d.iter_mut() {
                let s = v.max(0.0);
                *v = s * s;
            }
            planes.push(p);
        }
        Image::from_planes(&planes[0], &planes[1], &planes[2])
    }
}

/// Blurred with the five-tap binomial kernel and halved.
fn reduce(p: &Plane) -> Plane {
    const K: [f32; 5] = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    let (w, h) = (p.w, p.h);
    let (nw, nh) = ((w + 1) / 2, (h + 1) / 2);
    let mut rows = Plane::new(nw, h);
    for y in 0..h {
        let row = &p.d[y * w..(y + 1) * w];
        for x in 0..nw {
            let mut s = 0.0;
            for (k, kv) in K.iter().enumerate() {
                let sx = (2 * x as i32 + k as i32 - 2).clamp(0, w as i32 - 1) as usize;
                s += row[sx] * kv;
            }
            rows.d[y * nw + x] = s;
        }
    }
    let mut out = Plane::new(nw, nh);
    for y in 0..nh {
        for (k, kv) in K.iter().enumerate() {
            let sy = (2 * y as i32 + k as i32 - 2).clamp(0, h as i32 - 1) as usize;
            let src = &rows.d[sy * nw..(sy + 1) * nw];
            for (o, s) in out.d[y * nw..(y + 1) * nw].iter_mut().zip(src) {
                *o += *s * kv;
            }
        }
    }
    out
}

/// Back up to `w`×`h`, each pixel read halfway between its parents. Whatever
/// this does, the pyramid undoes it exactly, since the detail at each level
/// is measured against it.
fn expand(p: &Plane, w: usize, h: usize) -> Plane {
    let mut out = Plane::new(w, h);
    for y in 0..h {
        let sy = y as f32 * 0.5;
        for x in 0..w {
            out.d[y * w + x] = kit::sample_bilinear(p, x as f32 * 0.5, sy);
        }
    }
    out
}

// -------------------------------------------------------------------------
// half floats, for the median's copies
// -------------------------------------------------------------------------

const HALF_NAN: u16 = 0x7e00;

/// IEEE half precision, rounded to nearest. Plenty for linear light: eleven
/// bits of mantissa at every level, where a raw file has fourteen in all.
pub fn to_half(v: f32) -> u16 {
    if v.is_nan() {
        return HALF_NAN;
    }
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let a = v.abs();
    if a >= 65520.0 {
        return sign | 0x7c00;
    }
    if a < 6.103_515_6e-5 {
        // Subnormal: a multiple of 2^-24.
        return sign | (a * 16_777_216.0).round() as u16;
    }
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    let mut half = ((exp as u32) << 10) | (mantissa >> 13);
    // Round to nearest, ties to even.
    let rest = mantissa & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && half & 1 == 1) {
        half += 1;
    }
    sign | half as u16
}

pub fn from_half(h: u16) -> f32 {
    // Built from the bits: called for every reading of every frame the median
    // keeps, so no powers computed.
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as u32;
    let mantissa = (h & 0x3ff) as u32;
    match exp {
        0 => {
            let v = mantissa as f32 * 5.960_464_5e-8;
            if sign != 0 { -v } else { v }
        }
        31 => f32::from_bits(sign | 0x7f80_0000 | (mantissa << 13)),
        _ => f32::from_bits(sign | ((exp + 112) << 23) | (mantissa << 13)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Meta;

    // Big enough for the aligner's pyramid to reach a few pixels.
    const W: usize = 320;
    const H: usize = 240;

    /// A scene with something to line up on everywhere: soft discs of every
    /// size scattered over a gentle ramp.
    pub fn scene(x: f32, y: f32) -> f32 {
        let mut v = 0.05 + 0.1 * (x / W as f32) + 0.05 * (y / H as f32);
        let mut state = 0x9e37_79b9u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32
        };
        for _ in 0..40 {
            let (bx, by, r, a) = (next() * W as f32, next() * H as f32, 3.0 + next() * 12.0, next());
            let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt();
            v += 0.15 * a * kit::smoothstep(r + 1.5, r - 1.5, d);
        }
        v
    }

    fn frame(name: &str, mut f: impl FnMut(usize, usize) -> f32) -> Frame {
        let mut img = Image::new(W, H);
        for y in 0..H {
            for x in 0..W {
                let v = f(x, y);
                for c in 0..3 {
                    img.d[(y * W + x) * 3 + c] = v;
                }
            }
        }
        Frame {
            name: name.into(),
            linear: img,
            meta: Meta { shutter_s: 0.01, iso: 100.0, aperture: 8.0, ..Default::default() },
            exif: Vec::new(),
        }
    }

    /// Noise that is the same every run.
    fn noise(seed: u32) -> impl FnMut() -> f32 {
        let mut state = seed.wrapping_mul(2_654_435_761).max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        }
    }

    fn noisy(seed: u32) -> Frame {
        let mut n = noise(seed);
        let mut values = vec![0.0; W * H];
        for v in values.iter_mut() {
            *v = n() * 0.08;
        }
        frame(&format!("n{seed}"), |x, y| scene(x as f32, y as f32) + values[y * W + x])
    }

    fn residual(img: &Image) -> f32 {
        let mut sum = 0.0;
        for y in 0..H {
            for x in 0..W {
                sum += (img.d[(y * W + x) * 3] - scene(x as f32, y as f32)).powi(2);
            }
        }
        (sum / (W * H) as f32).sqrt()
    }

    fn stack(mode: Mode, align: bool, frames: Vec<Frame>) -> (Image, Notes) {
        let mut s = Stacker::new(Options::new(mode, align));
        for f in frames {
            s.add(f);
        }
        let (img, _, notes) = s.finish().expect("stack");
        (img, notes)
    }

    #[test]
    fn half_floats_round_trip() {
        for v in [0.0f32, 1.0, 0.5, 1e-5, 3.0e-4, 0.123_456, 7.5, 1000.0] {
            let back = from_half(to_half(v));
            assert!((back - v).abs() <= v.abs() / 1024.0 + 6e-8, "{v} came back as {back}");
        }
        assert!(from_half(HALF_NAN).is_nan());
    }

    #[test]
    fn the_average_has_less_noise() {
        let frames: Vec<Frame> = (1..=9).map(noisy).collect();
        let single = residual(&frames[0].linear);
        let (out, notes) = stack(Mode::Mean, false, frames);
        assert_eq!(notes.used, 9);
        let stacked = residual(&out);
        // Nine frames: a third of the noise.
        assert!(stacked < single * 0.4, "{single} -> {stacked}");
    }

    #[test]
    fn the_median_loses_what_passed_through() {
        let frames: Vec<Frame> = (0..5).map(|k| frame(&format!("{k}"), |x, y| {
            // Someone walking across: a bright block in a different place each time.
            let at = 20 + k * 25;
            if (at..at + 14).contains(&x) && (40..90).contains(&y) { 0.9 } else {
                scene(x as f32, y as f32)
            }
        })).collect();
        let (median, _) = stack(Mode::Median, false, frames);
        assert!(residual(&median) < 1e-3, "the walker should be gone: {}", residual(&median));
    }

    #[test]
    fn the_median_keeps_to_its_budget() {
        let mut s = Stacker::new(Options { budget: W * H * 6 * 3, ..Options::new(Mode::Median, false) });
        for k in 0..5 {
            s.add(noisy(k + 1));
        }
        let (_, _, notes) = s.finish().unwrap();
        assert_eq!(notes.used, 3);
        let told = notes.findings.iter().find(|f| f.code == "stack_memory").expect("told");
        assert_eq!(told.frames, vec![3, 4]);
    }

    #[test]
    fn lighten_keeps_every_star() {
        let frames: Vec<Frame> = (0..6).map(|k| frame(&format!("{k}"), |x, y| {
            if x == 20 + k * 10 && y == 30 { 1.0 } else { 0.02 }
        })).collect();
        let (out, _) = stack(Mode::Lighten, false, frames);
        for k in 0..6 {
            assert_eq!(out.d[(30 * W + 20 + k * 10) * 3], 1.0, "star {k} should stay");
        }
        assert_eq!(out.d[(60 * W + 60) * 3], 0.02, "and the sky stays the sky");
    }

    #[test]
    fn a_shifted_frame_is_lined_up_before_averaging() {
        let mut frames: Vec<Frame> = (1..=4).map(noisy).collect();
        frames[0] = frame("still", |x, y| scene(x as f32, y as f32));
        // The last one moved: what was at x is now at x + 4.
        frames[3] = frame("moved", |x, y| scene(x as f32 - 4.0, y as f32 + 2.0));
        let (out, notes) = stack(Mode::Mean, true, frames);
        assert_eq!(notes.added[3].shift, (4, -2));
        let aligned = residual(&out);
        assert!(aligned < 0.03, "the moved frame should have been put back: {aligned}");
    }

    #[test]
    fn a_different_picture_is_left_out() {
        let mut n = noise(99);
        let mut frames: Vec<Frame> = (1..=3).map(noisy).collect();
        frames.push(frame("elsewhere", |_, _| n() + 0.5));
        let (_, notes) = stack(Mode::Mean, true, frames);
        assert_eq!(notes.used, 3);
        assert_eq!(notes.added[3].left_out, Some("no_correspondence"));
    }

    #[test]
    fn breathing_is_measured() {
        let anchor = frame("a", |x, y| scene(x as f32, y as f32));
        let (cx, cy) = ((W as f32 - 1.0) * 0.5, (H as f32 - 1.0) * 0.5);
        // The same scene 2.4 % larger: a point at x is shown at
        // centre + 1.024·(x − centre), which is where the warp has to read.
        let grown = frame("b", |x, y| scene(cx + (x as f32 - cx) / 1.024,
                                             cy + (y as f32 - cy) / 1.024));
        let s = fit_scale(&small_grey(&anchor.linear), &small_grey(&grown.linear));
        assert!((s - 1.024).abs() < 0.002, "found {s}");
    }

    /// Blurred with a box, to stand in for being out of focus.
    fn defocused(f: impl Fn(f32, f32) -> f32, x: usize, y: usize) -> f32 {
        let mut sum = 0.0;
        for dy in -3..=3 {
            for dx in -3..=3 {
                sum += f(x as f32 + dx as f32, y as f32 + dy as f32);
            }
        }
        sum / 49.0
    }

    #[test]
    fn a_focus_stack_is_sharp_everywhere() {
        // Fine detail everywhere, which blurring all but removes.
        let detail = |x: f32, y: f32| 0.2 + 0.15 * (x * 1.7).sin() * (y * 1.3).cos();
        // Sharp on the left in one, on the right in the other.
        let near = frame("near", |x, y| if x < W / 2 { detail(x as f32, y as f32) }
                                        else { defocused(detail, x, y) });
        let far = frame("far", |x, y| if x >= W / 2 { detail(x as f32, y as f32) }
                                      else { defocused(detail, x, y) });
        let (out, _) = stack(Mode::Focus, false, vec![near, far]);
        let mut worst = 0.0f32;
        let mut sum = 0.0;
        for y in 8..H - 8 {
            for x in 8..W - 8 {
                if x.abs_diff(W / 2) < 6 {
                    continue;
                }
                let e = (out.d[(y * W + x) * 3] - detail(x as f32, y as f32)).abs();
                worst = worst.max(e);
                sum += e;
            }
        }
        let mean = sum / ((W - 16 - 11) * (H - 16)) as f32;
        assert!(mean < 0.01, "the stack should carry the sharp half of each: mean {mean}, worst {worst}");
    }

    #[test]
    fn a_breathing_focus_stack_is_lined_up() {
        let (cx, cy) = ((W as f32 - 1.0) * 0.5, (H as f32 - 1.0) * 0.5);
        let near = frame("near", |x, y| if x < W / 2 { scene(x as f32, y as f32) }
                                        else { defocused(scene, x, y) });
        // Focused further away, which on this lens shows the scene 1.5 % larger
        // and a little to the side.
        let at = |x: usize, y: usize| (cx + (x as f32 - 3.0 - cx) / 1.015, cy + (y as f32 - cy) / 1.015);
        let far = frame("far", |x, y| {
            let (sx, sy) = at(x, y);
            if sx >= W as f32 / 2.0 { scene(sx, sy) } else { defocused(scene, sx as usize, sy as usize) }
        });
        let (out, notes) = stack(Mode::Focus, true, vec![near, far]);
        assert!((notes.added[1].scale - 1.015).abs() < 0.003, "scale {}", notes.added[1].scale);
        let mut sum = 0.0;
        let mut count = 0;
        for y in 20..H - 20 {
            for x in W / 2 + 10..W - 20 {
                sum += (out.d[(y * W + x) * 3] - scene(x as f32, y as f32)).abs();
                count += 1;
            }
        }
        let mean = sum / count as f32;
        assert!(mean < 0.01, "the far half should come from the far frame, lined up: {mean}");
    }

    #[test]
    fn exposures_are_put_on_one_scale() {
        let bright = frame("bright", |x, y| scene(x as f32, y as f32));
        let mut dark = frame("dark", |x, y| scene(x as f32, y as f32) * 0.5);
        dark.meta.shutter_s = 0.005;
        let (out, notes) = stack(Mode::Mean, false, vec![bright, dark]);
        assert!((notes.added[1].stops + 1.0).abs() < 0.01);
        assert!(residual(&out) < 1e-3);
    }
}

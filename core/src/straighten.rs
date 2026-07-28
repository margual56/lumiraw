//! What would make this picture level, and its verticals upright.

use crate::geometry::Rect;
use crate::ops::{self, Image};
use serde_json::{json, Value};

/// Long edge the measurement runs at.
const WORK_EDGE: usize = 800;

/// How much of the frame counts as an edge.
const EDGE_QUANTILE: f64 = 0.90;

/// The scale the gradient is measured at, in pixels of the working frame.
const EDGE_SIGMA: f32 = 1.4;

/// Within this of horizontal, or of vertical, an edge joins that family.
const FAMILY_DEGREES: f32 = 30.0;

/// A tilt larger than this is a decision rather than a mistake, and
/// "straighten" is the wrong word for undoing it.
const MAX_ROLL_DEGREES: f32 = 12.0;

/// Votes are gathered into bins this wide, and the peak is taken as the
/// weighted mean of everything within `ROLL_WINDOW` of the fullest bin.
const ROLL_BIN_DEGREES: f32 = 0.25;
const ROLL_WINDOW_DEGREES: f32 = 1.5;

/// Below this share of the frame's edge weight, a family is too thin to
/// answer with.
const MIN_SUPPORT: f32 = 0.05;

/// How much taller than flat the winning direction has to be before there is
/// said to be a dominant direction at all.
const MIN_PROMINENCE: f64 = 2.5;

/// The prominence at which the measurement is as good as it is going to get,
/// for turning a ratio into the 0 to 1 the interface shows.
const SURE_PROMINENCE: f64 = 6.0;

/// A tilt smaller than this is not worth a sentence.
const MIN_ROLL_DEGREES: f64 = 0.3;
/// Likewise for the verticals, in the units the shift slider uses.
const MIN_PERSPECTIVE: f64 = 0.02;

/// The shift slider's own range. A convergence past this is real, but the
/// correction for it is not something this tool can offer.
const MAX_PERSPECTIVE: f64 = 0.6;

/// Within this of vertical, a cell counts toward the vanishing point.
const VERTICAL_DEGREES: f32 = 14.0;

/// Pairs closer together than this fraction of the frame's width are skipped
/// when taking pairwise slopes.
const MIN_BASELINE: f64 = 0.15;

/// Fewer usable pairs than this and the median of them means nothing.
const MIN_PAIRS: usize = 120;

/// The block the vanishing point's orientations are averaged over.
const CELL: usize = 24;

/// How lopsided a cell's structure tensor has to be before the cell is treated
/// as holding one line.
const MIN_COHERENCE: f64 = 0.72;

/// Fewer coherent cells than this and there is no fan, only a handful of
/// angles that a point can always be threaded through.
const MIN_CELLS: usize = 24;

/// How much better a vanishing point has to explain the vertical family than
/// "they are already parallel" does, before it is believed.
const MODEL_MARGIN: f64 = 0.7;

/// What the framing step should offer, and how sure it is.
#[derive(Clone, Copy, Debug, Default)]
pub struct Proposal {
    /// Degrees to add to `Framing::angle` to level the picture.
    pub angle: f64,
    pub angle_confidence: f32,
    /// To add to `Framing::perspective_v` to stand the verticals up.
    pub perspective_v: f64,
    pub perspective_confidence: f32,
    /// Share of the frame's edge weight in each family, which is what decides
    /// whether there was anything to measure.
    pub horizontal_support: f32,
    pub vertical_support: f32,
}

impl Proposal {
    /// Is either half of this worth putting in front of somebody?
    pub fn worth_offering(&self) -> bool {
        self.offers_angle() || self.offers_perspective()
    }
    pub fn offers_angle(&self) -> bool {
        self.angle.abs() >= MIN_ROLL_DEGREES && self.angle_confidence > 0.0
    }
    pub fn offers_perspective(&self) -> bool {
        self.perspective_v.abs() >= MIN_PERSPECTIVE && self.perspective_confidence > 0.0
    }

    pub fn to_json(&self) -> Value {
        json!({
            "proposed": self.worth_offering(),
            "angle": if self.offers_angle() { round(self.angle, 2) } else { 0.0 },
            "angle_confidence": round(self.angle_confidence as f64, 2),
            "perspective_v": if self.offers_perspective() {
                round(self.perspective_v, 3)
            } else {
                0.0
            },
            "perspective_confidence": round(self.perspective_confidence as f64, 2),
            "support": {"horizontal": round(self.horizontal_support as f64, 3),
                        "vertical": round(self.vertical_support as f64, 3)},
        })
    }
}

/// A prominence ratio as the 0 to 1 the interface shows.
fn confidence_from(prominence: f64) -> f32 {
    let t = ((prominence - MIN_PROMINENCE) / (SURE_PROMINENCE - MIN_PROMINENCE)).clamp(0.0, 1.0);
    (0.3 + 0.7 * t) as f32
}

fn round(v: f64, places: u32) -> f64 {
    let f = 10f64.powi(places as i32);
    (v * f).round() / f
}

/// How the working frame sits inside the frame the homography transforms.
struct Frame {
    /// Working pixels to view pixels.
    scale: f64,
    /// Where the view starts inside the full frame.
    origin: (f64, f64),
    work_centre: (f64, f64),
    img_centre: (f64, f64),
    half_diag: f64,
}

impl Frame {
    /// A point in working-frame centred pixels, in full-frame centred pixels.
    fn to_frame(&self, p: (f64, f64)) -> (f64, f64) {
        let view_x = (p.0 + self.work_centre.0) * self.scale;
        let view_y = (p.1 + self.work_centre.1) * self.scale;
        (view_x + self.origin.0 - self.img_centre.0,
         view_y + self.origin.1 - self.img_centre.1)
    }
}

/// One edge pixel's vote.
struct Vote {
    /// Where it is, in pixels from the centre of the working frame.
    x: f32,
    y: f32,
    /// The direction the edge runs in, normalised, pointing down the frame.
    dx: f32,
    dy: f32,
    /// How much it should count: the gradient's own strength.
    weight: f32,
    /// How far this edge is from level, in degrees, folded so that a vertical
    /// and a horizontal disagreeing by the same amount vote together.
    tilt: f32,
    vertical: bool,
}

/// Measure the frame and say what would straighten it.
pub fn propose(img: &Image, rect: Option<Rect>) -> Option<Proposal> {
    let bounds = crate::geometry::view_bounds(img.w, img.h, rect);
    let view = crate::geometry::stats_view_img(img, rect);
    let work = ops::thumbnail(&view, WORK_EDGE);
    if work.w < 64 || work.h < 64 {
        return None;
    }
    // Angles survive a crop and a resize, but a vanishing point does not.
    let (ox, oy) = bounds.map(|(x, y, _, _)| (x as f64, y as f64)).unwrap_or((0.0, 0.0));
    let frame = Frame {
        scale: view.w as f64 / work.w as f64,
        origin: (ox, oy),
        work_centre: ((work.w as f64 - 1.0) / 2.0, (work.h as f64 - 1.0) / 2.0),
        img_centre: ((img.w as f64 - 1.0) / 2.0, (img.h as f64 - 1.0) / 2.0),
        half_diag: ((img.w * img.w + img.h * img.h) as f64).sqrt() / 2.0,
    };
    let votes = gather(&work);
    if votes.is_empty() {
        return None;
    }

    let total: f32 = votes.iter().map(|v| v.weight).sum();
    if total <= 0.0 {
        return None;
    }
    let horizontal_support =
        votes.iter().filter(|v| !v.vertical).map(|v| v.weight).sum::<f32>() / total;
    let vertical_support = 1.0 - horizontal_support;

    let mut out = Proposal {
        horizontal_support,
        vertical_support,
        ..Proposal::default()
    };

    if let Some((tilt, prominence)) = roll(&votes, total) {
        out.angle = -(tilt as f64);
        out.angle_confidence = confidence_from(prominence as f64);
    }
    // The vanishing point is fitted from pooled cells rather than from the
    // votes above, for a reason worth writing down.
    if let Some((perspective, agreement)) = verticals(&cells(&work), &frame, vertical_support) {
        out.perspective_v = perspective;
        out.perspective_confidence = agreement;
    }
    Some(out)
}

/// The frame in blocks, each with the one orientation that runs through it.
fn cells(work: &Image) -> Vec<Vote> {
    let luma = ops::gaussian_blur(&ops::luminance(work), EDGE_SIGMA);
    let (w, h) = (luma.w, luma.h);
    let at = |x: usize, y: usize| luma.d[y * w + x];
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let family = VERTICAL_DEGREES.to_radians();
    let right_angle = std::f32::consts::FRAC_PI_2;

    let mut out = Vec::new();
    let mut y0 = 1;
    while y0 + CELL < h - 1 {
        let mut x0 = 1;
        while x0 + CELL < w - 1 {
            let (mut jxx, mut jxy, mut jyy) = (0.0f64, 0.0f64, 0.0f64);
            for y in y0..y0 + CELL {
                for x in x0..x0 + CELL {
                    let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                        - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
                    let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                        - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
                    jxx += (gx * gx) as f64;
                    jxy += (gx * gy) as f64;
                    jyy += (gy * gy) as f64;
                }
            }
            x0 += CELL;
            let trace = jxx + jyy;
            if trace <= 1e-9 {
                continue;
            }
            let diff = ((jxx - jyy) * (jxx - jyy) + 4.0 * jxy * jxy).sqrt();
            // One dominant orientation, or a corner and a texture pretending
            // to be one.
            let coherence = diff / trace;
            if coherence < MIN_COHERENCE {
                continue;
            }
            // Principal gradient direction, and the edge runs across it.
            let theta = 0.5 * (2.0 * jxy).atan2(jxx - jyy);
            let (mut dx, mut dy) = (-theta.sin() as f32, theta.cos() as f32);
            if dy < 0.0 {
                dx = -dx;
                dy = -dy;
            }
            let phi = dy.atan2(dx);
            let phi = if phi > right_angle { phi - std::f32::consts::PI } else { phi };
            if phi.abs() < right_angle - family {
                continue;
            }
            out.push(Vote {
                x: (x0 - CELL) as f32 + CELL as f32 / 2.0 - cx,
                y: y0 as f32 + CELL as f32 / 2.0 - cy,
                dx,
                dy,
                // The coherent part of the energy: a long clean edge across
                // the cell counts for more than a faint one.
                weight: (diff * coherence) as f32,
                tilt: (phi - phi.signum() * right_angle).to_degrees(),
                vertical: true,
            });
        }
        y0 += CELL;
    }
    out
}

/// Every pixel with a strong enough gradient, as a vote.
fn gather(work: &Image) -> Vec<Vote> {
    // Blurred first, and not only against noise.
    let luma = ops::gaussian_blur(&ops::luminance(work), EDGE_SIGMA);
    let (w, h) = (luma.w, luma.h);
    let at = |x: usize, y: usize| luma.d[y * w + x];

    // Sobel, which is a derivative and a smoothing in one and is enough here:
    // the question is which way an edge runs, not exactly where it is.
    let mut grads = Vec::with_capacity((w - 2) * (h - 2));
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            grads.push((x, y, gx, gy));
        }
    }
    let mut mags: Vec<f32> = grads.iter().map(|(_, _, gx, gy)| gx.hypot(*gy)).collect();
    let cut = ops::percentile(&mags, EDGE_QUANTILE);
    // A frame of flat sky has a ninetieth percentile too, and it is noise.
    let cut = cut.max(0.10);
    mags.clear();
    mags.shrink_to_fit();

    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let family = FAMILY_DEGREES.to_radians();
    let right_angle = std::f32::consts::FRAC_PI_2;
    let mut votes = Vec::new();
    for (x, y, gx, gy) in grads {
        let weight = gx.hypot(gy);
        if weight < cut {
            continue;
        }
        // The edge runs across the gradient, not along it.
        let (mut dx, mut dy) = (-gy, gx);
        // A line has no direction, only an orientation, so fold everything
        // into the half turn that points down the frame.
        if dy < 0.0 {
            dx = -dx;
            dy = -dy;
        }
        let len = dx.hypot(dy).max(1e-6);
        let (dx, dy) = (dx / len, dy / len);
        // Angle from the x axis, in (-90, 90]: 0 is a horizontal edge, +-90 a
        // vertical one.
        let phi = dy.atan2(dx);
        let phi = if phi > right_angle { phi - std::f32::consts::PI } else { phi };

        let (tilt, vertical) = if phi.abs() <= family {
            (phi, false)
        } else if phi.abs() >= right_angle - family {
            // A vertical tilted by the same roll reads 90 degrees away from
            // one that is horizontal, so fold it back on top.
            (phi - phi.signum() * right_angle, true)
        } else {
            continue;
        };
        votes.push(Vote {
            x: x as f32 - cx,
            y: y as f32 - cy,
            dx,
            dy,
            weight,
            tilt: tilt.to_degrees(),
            vertical,
        });
    }
    votes
}

/// The roll, from the fullest bin of the folded tilt histogram.
fn roll(votes: &[Vote], total: f32) -> Option<(f32, f32)> {
    let bins = (2.0 * FAMILY_DEGREES / ROLL_BIN_DEGREES).ceil() as usize + 1;
    let mut hist = vec![0f32; bins];
    let index = |tilt: f32| {
        (((tilt + FAMILY_DEGREES) / ROLL_BIN_DEGREES).round() as isize).clamp(0, bins as isize - 1)
            as usize
    };
    for v in votes {
        hist[index(v.tilt)] += v.weight;
    }
    // Look for the fullest window rather than the fullest bin.
    let window = (ROLL_WINDOW_DEGREES / ROLL_BIN_DEGREES).round() as isize;
    let mut best = (0usize, f32::MIN);
    for centre in 0..bins {
        let mut sum = 0.0;
        for offset in -window..=window {
            let i = centre as isize + offset;
            if i >= 0 && i < bins as isize {
                sum += hist[i as usize];
            }
        }
        if sum > best.1 {
            best = (centre, sum);
        }
    }
    let peak = best.0 as f32 * ROLL_BIN_DEGREES - FAMILY_DEGREES;

    // Sub-bin precision, from the weighted mean of what is actually in the
    // window that won.
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for v in votes {
        if (v.tilt - peak).abs() <= ROLL_WINDOW_DEGREES {
            num += (v.tilt * v.weight) as f64;
            den += v.weight as f64;
        }
    }
    if den <= 0.0 {
        return None;
    }
    let tilt = (num / den) as f32;
    if tilt.abs() > MAX_ROLL_DEGREES {
        return None;
    }

    // How much taller the peak is than a flat histogram of the same weight
    // would be over a window of the same width.
    let span = (2.0 * FAMILY_DEGREES) as f64;
    let window = (2.0 * ROLL_WINDOW_DEGREES) as f64;
    let expected = total as f64 * window / span;
    let prominence = den / expected.max(1e-9);
    if prominence < MIN_PROMINENCE {
        return None;
    }
    Some((tilt, prominence as f32))
}

/// Where the verticals meet, and what that is worth as a shift.
fn verticals(cells: &[Vote], frame: &Frame, support: f32) -> Option<(f64, f32)> {
    if support < MIN_SUPPORT || cells.len() < MIN_CELLS {
        return None;
    }
    // Slope of the line in each cell, against where the cell is.
    let points: Vec<(f64, f64)> = cells
        .iter()
        .map(|c| (c.x as f64, (c.dx / c.dy.max(1e-6)) as f64))
        .collect();

    // Only pairs far enough apart to have a slope worth taking: over a short
    // baseline the difference between two measured angles is mostly noise.
    let spread = frame.work_centre.0 * 2.0;
    let min_base = spread * MIN_BASELINE;

    // Two refinements of the linearisation.
    let mut y_v = f64::NAN;
    let mut fit = (0.0f64, 0.0f64);
    for pass in 0..3 {
        let scaled: Vec<(f64, f64)> = if pass == 0 {
            points.clone()
        } else {
            cells
                .iter()
                .zip(points.iter())
                .map(|(c, (x, m))| (*x, m * (1.0 - c.y as f64 / y_v)))
                .collect()
        };
        fit = theil_sen(&scaled, min_base)?;
        if fit.1.abs() < 1e-12 {
            return None;
        }
        let next = -1.0 / fit.1;
        if !next.is_finite() {
            return None;
        }
        y_v = next;
    }

    // Back into the frame the homography is built for, since the fit ran on a
    // thumbnail of a crop of it.
    let x_v = -fit.0 / fit.1;
    let (_, y_full) = frame.to_frame((x_v, y_v));
    let half_diag = frame.half_diag;
    // A vanishing point inside the picture is not a vanishing point, it is a
    // failed fit; anything the slider cannot reach is not an offer.
    if y_full.abs() < half_diag * 0.5 {
        return None;
    }
    let perspective = -half_diag / y_full;
    if perspective.abs() > MAX_PERSPECTIVE {
        return None;
    }

    // Does a sloped model actually beat a flat one?
    let flat = median(&points.iter().map(|(_, m)| *m).collect::<Vec<_>>());
    let sloped: Vec<f64> = points
        .iter()
        .map(|(x, m)| (m - (fit.0 + fit.1 * x)).abs())
        .collect();
    let flats: Vec<f64> = points.iter().map(|(_, m)| (m - flat).abs()).collect();
    let (sloped_err, flat_err) = (median(&sloped), median(&flats));
    if sloped_err > flat_err * MODEL_MARGIN {
        return None;
    }

    // Confidence from how much better it did and how straight the lines were.
    let earned = (1.0 - sloped_err / flat_err.max(1e-9)).clamp(0.0, 1.0);
    let tightness = (1.0 - sloped_err.atan().to_degrees() / 3.0).clamp(0.0, 1.0);
    let agreement = (0.3 + 0.7 * earned * tightness) as f32 * support.min(1.0).sqrt();
    if agreement <= 0.05 {
        return None;
    }
    Some((perspective, agreement))
}

/// Median of the pairwise slopes, and the intercept that goes with it.
fn theil_sen(points: &[(f64, f64)], min_base: f64) -> Option<(f64, f64)> {
    let mut slopes = Vec::new();
    for (i, a) in points.iter().enumerate() {
        for b in points.iter().skip(i + 1) {
            let dx = b.0 - a.0;
            if dx.abs() < min_base {
                continue;
            }
            slopes.push((b.1 - a.1) / dx);
        }
    }
    if slopes.len() < MIN_PAIRS {
        return None;
    }
    let slope = median(&slopes);
    // The intercept that puts the line through the middle of the data, which
    // is the usual companion to a Theil-Sen slope.
    let residuals: Vec<f64> = points.iter().map(|(x, y)| y - slope * x).collect();
    Some((median(&residuals), slope))
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{self, Framing};

    /// A frame of horizontal and vertical bars, which is the easiest thing
    /// there is to level and therefore the first thing that has to work.
    fn grid(w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let bar = x % 40 < 6 || y % 40 < 6;
                let v = if bar { 0.85 } else { 0.12 };
                for c in 0..3 {
                    img.d[(y * w + x) * 3 + c] = v;
                }
            }
        }
        img
    }

    /// The sign has to be right, or the button makes it worse.
    #[test]
    fn a_tilted_frame_is_offered_the_angle_that_undoes_it() {
        for tilt in [-4.0f64, -1.5, 2.0, 5.5] {
            let source = grid(600, 400);
            let tilted = geometry::apply(
                &source,
                &Framing { angle: tilt, auto_fit: false, ..Framing::default() },
                false,
            );
            // Measure the middle, away from the empty wedges the rotation left.
            let inset = Some((0.25f32, 0.25, 0.5, 0.5));
            let p = propose(&tilted, inset).expect("a grid has lines");
            assert!(p.offers_angle(), "nothing offered for a {tilt} degree tilt: {p:?}");
            assert!(
                (p.angle + tilt).abs() < 0.35,
                "offered {:.2} to undo {tilt}, which is the wrong way or the wrong size",
                p.angle
            );
        }
    }

    /// And the answer for a frame that is already level is no answer.
    #[test]
    fn a_level_frame_is_left_alone() {
        let p = propose(&grid(600, 400), None).expect("a grid has lines");
        assert!(!p.offers_angle(), "offered {:.2} degrees on a level grid", p.angle);
    }

    /// Converging verticals, made by the same transform that has to undo them.
    #[test]
    fn converging_verticals_are_offered_the_shift_that_stands_them_up() {
        for shift in [0.12f64, 0.25] {
            let source = grid(900, 600);
            // A negative shift leans the verticals in at the top, which is
            // what tilting a camera up at a building does.
            let leaning = geometry::apply(
                &source,
                &Framing { perspective_v: -shift, ..Framing::default() },
                true,
            );
            let p = propose(&leaning, None).expect("a grid has lines");
            assert!(p.offers_perspective(), "nothing offered for a {shift} lean: {p:?}");
            assert!(
                p.perspective_v > 0.0,
                "offered {:.3}, which leans it further",
                p.perspective_v
            );
            // Applying it has to take the lean out, not merely change it.
            let fixed = geometry::apply(
                &leaning,
                &Framing { perspective_v: p.perspective_v, ..Framing::default() },
                true,
            );
            let after = propose(&fixed, None).expect("still a grid");
            assert!(
                after.perspective_v.abs() < shift * 0.35,
                "{shift} lean, offered {:.3}, {:.3} still left afterwards",
                p.perspective_v,
                after.perspective_v
            );
        }
    }

    /// Verticals that are already parallel must not be handed a shift.
    #[test]
    fn parallel_verticals_are_left_alone() {
        let p = propose(&grid(900, 600), None).expect("a grid has lines");
        assert!(
            !p.offers_perspective(),
            "offered {:.3} to a grid that is already square",
            p.perspective_v
        );
    }

    /// A third of the near-vertical edges pointing somewhere else entirely,
    /// which is what a tree in front of a building amounts to.
    #[test]
    fn a_converging_frame_survives_distractors() {
        let shift = 0.2f64;
        let mut img = geometry::apply(
            &grid(900, 600),
            &Framing { perspective_v: -shift, ..Framing::default() },
            true,
        );
        // Bars leaning the other way, over the left third.
        let (w, h) = (img.w, img.h);
        for y in 0..h {
            for x in 0..w / 3 {
                let skew = x as isize - (y as isize * 3) / 10;
                if skew.rem_euclid(37) < 5 {
                    for c in 0..3 {
                        img.d[(y * w + x) * 3 + c] = 0.9;
                    }
                }
            }
        }
        let p = propose(&img, None).expect("still a grid");
        assert!(p.offers_perspective(), "distractors silenced it: {p:?}");
        assert!(
            p.perspective_v > 0.0,
            "offered {:.3}, which leans it further",
            p.perspective_v
        );
    }

    /// The estimate must not shrink toward zero.
    #[test]
    fn the_offer_is_not_attenuated() {
        for shift in [0.15f64, 0.3] {
            let leaning = geometry::apply(
                &grid(900, 600),
                &Framing { perspective_v: -shift, ..Framing::default() },
                true,
            );
            let p = propose(&leaning, None).expect("a grid has lines");
            let fixed = geometry::apply(
                &leaning,
                &Framing { perspective_v: p.perspective_v, ..Framing::default() },
                true,
            );
            let left = propose(&fixed, None).expect("still a grid");
            assert!(
                left.perspective_v.abs() < shift * 0.3,
                "{shift} lean: offered {:.3}, still {:.3} left, which is a shrunk estimate",
                p.perspective_v,
                left.perspective_v
            );
        }
    }

    /// Noise has edges in every direction and no lines at all.
    #[test]
    fn a_frame_with_no_lines_proposes_nothing() {
        let mut img = Image::new(400, 300);
        let mut state = 0x12345678u32;
        for v in img.d.iter_mut() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *v = state as f32 / u32::MAX as f32;
        }
        let p = propose(&img, None).expect("noise still has gradients");
        assert!(!p.worth_offering(), "offered something for noise: {p:?}");
    }
}

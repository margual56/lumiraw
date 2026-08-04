//! Tone curves: control points in, a lookup table out.

use crate::ops::{self, Image};
use serde_json::Value;

/// How many entries a baked curve holds.
pub const TABLE: usize = 256;

/// A curve, ready to apply.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    table: Vec<f32>,
    /// How many control points survived `prepare` and shaped the table.
    knots: usize,
}

impl Curve {
    /// The curve that changes nothing.
    pub fn identity() -> Curve {
        Curve { table: (0..TABLE).map(|i| i as f32 / (TABLE - 1) as f32).collect(), knots: 0 }
    }

    /// Whether this curve is the one that changes nothing, so a caller can skip
    /// a pass over twenty million pixels rather than multiply them all by one.
    pub fn is_identity(&self) -> bool {
        self.table.iter().enumerate().all(|(i, v)| {
            (v - i as f32 / (TABLE - 1) as f32).abs() < 1e-6
        })
    }

    /// The curve through these control points.
    pub fn new(points: &[(f32, f32)]) -> Curve {
        let knots = prepare(points);
        if knots.len() < 2 {
            return Curve::identity();
        }
        let slopes = monotone_slopes(&knots);
        let mut table = Vec::with_capacity(TABLE);
        for i in 0..TABLE {
            let x = i as f32 / (TABLE - 1) as f32;
            table.push(hermite(&knots, &slopes, x).clamp(0.0, 1.0));
        }
        Curve { table, knots: knots.len() }
    }

    /// The curve at one value, interpolating between table entries.
    #[inline]
    pub fn at(&self, x: f32) -> f32 {
        let t = x.clamp(0.0, 1.0) * (TABLE - 1) as f32;
        let i = t as usize;
        if i >= TABLE - 1 {
            return self.table[TABLE - 1];
        }
        let f = t - i as f32;
        self.table[i] * (1.0 - f) + self.table[i + 1] * f
    }

    /// How many control points shaped this curve, after the unreadable ones
    /// were dropped and any two at the same place were merged.
    pub fn knots(&self) -> usize {
        self.knots
    }

    /// The baked table, for a caller that wants to draw it.
    pub fn table(&self) -> &[f32] {
        &self.table
    }

    /// This curve followed by another.
    pub fn then(&self, next: &Curve) -> Curve {
        if next.is_identity() {
            return self.clone();
        }
        if self.is_identity() {
            return next.clone();
        }
        Curve {
            table: self.table.iter().map(|v| next.at(*v)).collect(),
            // The knots of the curve being shaped; the second is a transform
            // applied to it, not more handles on it.
            knots: self.knots,
        }
    }

    /// This curve, pulled back toward the identity.
    pub fn scaled(&self, strength: f32) -> Curve {
        let s = strength.clamp(0.0, 1.0);
        Curve {
            table: self.table.iter().enumerate().map(|(i, v)| {
                let x = i as f32 / (TABLE - 1) as f32;
                x * (1.0 - s) + v * s
            }).collect(),
            knots: self.knots,
        }
    }
}

/// Control points, sorted, de-duplicated, clamped, and forced to rise.
fn prepare(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut p: Vec<(f32, f32)> = points
        .iter()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
        .collect();
    p.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(p.len());
    for (x, y) in p {
        // Two points at the same x are a vertical step, which has no slope and
        // no meaning; the later one wins.
        if let Some(last) = out.last_mut() {
            if (x - last.0).abs() < 1e-6 {
                last.1 = y.max(last.1);
                continue;
            }
        }
        let floor = out.last().map(|(_, y)| *y).unwrap_or(0.0);
        out.push((x, y.max(floor)));
    }
    out
}

/// Fritsch-Carlson tangents: the standard way to make a cubic through monotone
/// data stay monotone.
fn monotone_slopes(k: &[(f32, f32)]) -> Vec<f32> {
    let n = k.len();
    let secant: Vec<f32> = (0..n - 1)
        .map(|i| (k[i + 1].1 - k[i].1) / (k[i + 1].0 - k[i].0))
        .collect();
    let mut m = Vec::with_capacity(n);
    m.push(secant[0]);
    for i in 1..n - 1 {
        m.push(0.5 * (secant[i - 1] + secant[i]));
    }
    m.push(secant[n - 2]);

    for i in 0..n - 1 {
        if secant[i].abs() < 1e-9 {
            // A flat run has to stay flat at both ends, or the cubic bulges
            // through it.
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let a = m[i] / secant[i];
        let b = m[i + 1] / secant[i];
        let sum = a * a + b * b;
        if sum > 9.0 {
            let tau = 3.0 / sum.sqrt();
            m[i] = tau * a * secant[i];
            m[i + 1] = tau * b * secant[i];
        }
    }
    m
}

/// The cubic Hermite value at `x`, flat outside the outermost points.
fn hermite(k: &[(f32, f32)], m: &[f32], x: f32) -> f32 {
    if x <= k[0].0 {
        return k[0].1;
    }
    if x >= k[k.len() - 1].0 {
        return k[k.len() - 1].1;
    }
    let i = match k.binary_search_by(|p| p.0.partial_cmp(&x).unwrap()) {
        Ok(hit) => return k[hit].1,
        Err(next) => next - 1,
    };
    let h = k[i + 1].0 - k[i].0;
    let t = (x - k[i].0) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * k[i].1
        + (t3 - 2.0 * t2 + t) * h * m[i]
        + (-2.0 * t3 + 3.0 * t2) * k[i + 1].1
        + (t3 - t2) * h * m[i + 1]
}

/// How far a region slider at full deflection moves its part of the range.
const REGION_REACH: f32 = 0.16;

/// The four region sliders, as a curve.
pub fn parametric(shadows: f32, darks: f32, lights: f32, highlights: f32) -> Curve {
    let weights = [shadows, darks, lights, highlights];
    if weights.iter().all(|w| w.abs() < 1e-4) {
        return Curve::identity();
    }
    let mut points = vec![(0.0f32, 0.0f32)];
    for (i, w) in weights.iter().enumerate() {
        let x = 0.125 + 0.25 * i as f32;
        points.push((x, x + w.clamp(-1.0, 1.0) * REGION_REACH));
    }
    points.push((1.0, 1.0));
    Curve::new(&points)
}

// -------------------------------------------------------------------------
// the stack the pipeline applies
// -------------------------------------------------------------------------

/// The four curves a grade is made of.
#[derive(Clone, Debug, PartialEq)]
pub struct Stack {
    composite: Curve,
    channel: [Curve; 3],
    /// Per channel: encoded input in, *linear* output out.
    baked: Vec<f32>,
    /// Whether this stack would change anything, worked out once.
    identity: bool,
}

impl Stack {
    /// The stack that changes nothing.
    pub fn identity() -> Stack {
        let i = Curve::identity();
        Stack::new(i.clone(), i.clone(), i.clone(), i, 1.0)
    }

    /// The stack these four curves make, faded by `strength`.
    pub fn new(composite: Curve, red: Curve, green: Curve, blue: Curve, strength: f32) -> Stack {
        let composite = composite.scaled(strength);
        let channel = [red.scaled(strength), green.scaled(strength), blue.scaled(strength)];
        let identity = composite.is_identity() && channel.iter().all(Curve::is_identity);
        let mut baked = Vec::with_capacity(3 * TABLE);
        for c in &channel {
            for i in 0..TABLE {
                let x = i as f32 / (TABLE - 1) as f32;
                baked.push(ops::srgb_decode_scalar(c.at(composite.at(x))));
            }
        }
        Stack { composite, channel, baked, identity }
    }

    /// Whether applying this would be a pass over every pixel to multiply it
    /// by one.
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// The composite curve, for drawing.
    pub fn composite(&self) -> &Curve {
        &self.composite
    }

    /// One channel's curve, for drawing. 0 is red, 1 green, 2 blue.
    pub fn channel(&self, c: usize) -> &Curve {
        &self.channel[c.min(2)]
    }

    /// One channel of one pixel: display-linear in, display-linear out.
    #[inline]
    fn map(&self, c: usize, v: f32) -> f32 {
        // Anything above white is already gone by the time a file is written,
        // because `output.rs` encodes with the same clamp.
        let t = ops::srgb_encode_scalar(v) * (TABLE - 1) as f32;
        let i = t as usize;
        let base = c * TABLE;
        if i >= TABLE - 1 {
            return self.baked[base + TABLE - 1];
        }
        let f = t - i as f32;
        self.baked[base + i] * (1.0 - f) + self.baked[base + i + 1] * f
    }

    /// The image with the stack applied, or the image itself if there is
    /// nothing to apply.
    pub fn apply(&self, img: &Image) -> Image {
        if self.identity {
            return img.clone();
        }
        let mut out = img.clone();
        for i in 0..img.w * img.h {
            for c in 0..3 {
                out.d[i * 3 + c] = self.map(c, out.d[i * 3 + c]);
            }
        }
        out
    }

    /// A short digest of what this stack does to a picture, for a cache key.
    pub fn fingerprint(&self) -> u64 {
        if self.identity {
            return 0;
        }
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for v in &self.baked {
            for byte in v.to_bits().to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(0x100_0000_01b3);
            }
        }
        h
    }

    /// How many control points shaped each curve, composite first.
    pub fn counts(&self) -> [usize; 4] {
        [self.composite.knots(), self.channel[0].knots(), self.channel[1].knots(),
         self.channel[2].knots()]
    }

    /// The stack a settings object asks for, or the identity if it asks for
    /// nothing.
    pub fn from_json(v: Option<&Value>) -> Stack {
        let Some(v) = v else { return Stack::identity() };
        let strength = v.get("strength").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
        // The region sliders run first and the dragged points second, so the
        // points are read on tones the sliders have already placed.
        let regions = v.get("regions");
        let region = |k: &str| regions.and_then(|r| r.get(k)).and_then(|x| x.as_f64())
                                      .unwrap_or(0.0) as f32;
        let bands = parametric(region("shadows"), region("darks"), region("lights"),
                               region("highlights"));
        Stack::new(
            bands.then(&Curve::new(&points(v, "rgb"))),
            Curve::new(&points(v, "r")),
            Curve::new(&points(v, "g")),
            Curve::new(&points(v, "b")),
            strength,
        )
    }
}

/// The five curves the editor shows, each exactly as it is edited.
pub fn edited(v: Option<&Value>) -> [Curve; 5] {
    let Some(v) = v else { return std::array::from_fn(|_| Curve::identity()) };
    let region = |k: &str| v.get("regions").and_then(|r| r.get(k)).and_then(|x| x.as_f64())
                            .unwrap_or(0.0) as f32;
    [
        parametric(region("shadows"), region("darks"), region("lights"),
                   region("highlights")),
        Curve::new(&points(v, "rgb")),
        Curve::new(&points(v, "r")),
        Curve::new(&points(v, "g")),
        Curve::new(&points(v, "b")),
    ]
}

/// One channel's control points out of a settings object.
fn points(v: &Value, key: &str) -> Vec<(f32, f32)> {
    let Some(a) = v.get(key).and_then(|x| x.as_array()) else { return Vec::new() };
    a.iter()
        .filter_map(|p| {
            let q = p.as_array()?;
            if q.len() < 2 {
                return None;
            }
            Some((q[0].as_f64()? as f32, q[1].as_f64()? as f32))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::s_curve;

    fn rises(c: &Curve) -> bool {
        c.table().windows(2).all(|w| w[1] >= w[0] - 1e-6)
    }

    /// No control points means no opinion, and an empty channel in a preset is
    /// the ordinary case rather than a mistake.
    #[test]
    fn nothing_to_say_leaves_the_picture_alone() {
        assert!(Curve::new(&[]).is_identity());
        assert!(Curve::new(&[(0.5, 0.7)]).is_identity(), "one point is not a curve");
        assert!(Curve::identity().is_identity());
    }

    /// The invariant the whole type exists to hold.
    #[test]
    fn a_curve_never_falls() {
        // Points that descend on purpose, which a dragged handle can produce.
        let folded = Curve::new(&[(0.0, 0.0), (0.3, 0.8), (0.6, 0.1), (1.0, 1.0)]);
        assert!(rises(&folded), "a folded input must still come back monotone");
        // And the ordinary shapes.
        for points in [
            vec![(0.0, 0.0), (0.25, 0.15), (0.75, 0.85), (1.0, 1.0)],
            vec![(0.0, 0.06), (0.5, 0.5), (1.0, 0.94)],
            vec![(0.0, 0.0), (0.2, 0.2), (0.4, 0.2), (1.0, 1.0)],
        ] {
            assert!(rises(&Curve::new(&points)), "{points:?} came back folded");
        }
    }

    /// A flat run has to stay flat. A plain cubic bulges through one, which on
    /// a tone curve is a bright band just above a shadow that was pinned down.
    #[test]
    fn a_flat_run_does_not_bulge() {
        let c = Curve::new(&[(0.0, 0.0), (0.3, 0.3), (0.6, 0.3), (1.0, 1.0)]);
        for i in 0..=30 {
            let x = 0.3 + 0.3 * i as f32 / 30.0;
            assert!((c.at(x) - 0.3).abs() < 1e-3,
                    "the run between 0.3 and 0.6 bulged to {} at {x}", c.at(x));
        }
    }

    /// The general form has to be able to say what the special case said, or
    /// replacing one with the other changes every photograph silently.
    #[test]
    fn it_can_express_the_curve_it_replaces() {
        for amount in [0.2f32, 0.45, 0.6] {
            for pivot in [0.35f32, 0.5, 0.62] {
                let points: Vec<(f32, f32)> = (0..=16)
                    .map(|i| {
                        let x = i as f32 / 16.0;
                        (x, s_curve(x, amount, pivot))
                    })
                    .collect();
                let c = Curve::new(&points);
                for i in 0..=100 {
                    let x = i as f32 / 100.0;
                    let want = s_curve(x, amount, pivot);
                    assert!((c.at(x) - want).abs() < 0.004,
                            "amount {amount} pivot {pivot} at {x}: {} against {want}", c.at(x));
                }
            }
        }
    }

    /// Fading a curve has to fade it, not reshape it.
    #[test]
    fn strength_fades_toward_the_identity() {
        let full = Curve::new(&[(0.0, 0.0), (0.25, 0.12), (0.75, 0.88), (1.0, 1.0)]);
        assert!(full.scaled(0.0).is_identity(), "nothing of it at zero");
        assert_eq!(full.scaled(1.0), full, "all of it at one");
        let half = full.scaled(0.5);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            let want = 0.5 * x + 0.5 * full.at(x);
            assert!((half.at(x) - want).abs() < 1e-5, "half strength is not half way at {x}");
        }
        assert!(rises(&half), "a faded curve is still a curve");
    }

    /// The ends are the ends: a curve that lifts the toe must still reach the
    /// white point it was given, or every picture loses its highlights.
    #[test]
    fn the_endpoints_are_honoured() {
        let c = Curve::new(&[(0.0, 0.08), (0.5, 0.5), (1.0, 0.93)]);
        assert!((c.at(0.0) - 0.08).abs() < 1e-4, "toe not held: {}", c.at(0.0));
        assert!((c.at(1.0) - 0.93).abs() < 1e-4, "shoulder not held: {}", c.at(1.0));
    }

    // ---------------------------------------------------------------------
    // the stack
    // ---------------------------------------------------------------------

    fn grey(v: f32) -> Image {
        let mut img = Image::new(1, 1);
        for c in 0..3 {
            img.d[c] = v;
        }
        img
    }

    /// A photograph nobody graded has to come out of this exactly as it went
    /// in.
    #[test]
    fn an_ungraded_frame_comes_back_untouched() {
        let stack = Stack::identity();
        assert!(stack.is_identity());
        assert_eq!(stack.fingerprint(), 0, "the identity must key as it did before curves");
        let img = grey(0.1837);
        let out = stack.apply(&img);
        assert_eq!(out.d, img.d, "an identity stack altered a pixel");
    }

    /// The claim the whole design rests on.
    #[test]
    fn a_curve_is_read_on_encoded_values() {
        let lifted = Curve::new(&[(0.0, 0.0), (0.5, 0.6), (1.0, 1.0)]);
        let stack = Stack::new(lifted, Curve::identity(), Curve::identity(), Curve::identity(), 1.0);
        let out = stack.apply(&grey(ops::srgb_decode_scalar(0.5)));
        let want = ops::srgb_decode_scalar(0.6);
        assert!((out.d[0] - want).abs() < 1e-3,
                "a pixel at 0.5 encoded came out {} rather than {want}", out.d[0]);
        assert!((out.d[0] - 0.6).abs() > 0.1,
                "0.6 landed in linear light, so the curve was applied in the wrong space");
    }

    /// What a lightness curve cannot do at any amount, and the reason there are
    /// four curves rather than one.
    #[test]
    fn a_channel_curve_shifts_hue_with_brightness() {
        let toe = Curve::new(&[(0.0, 0.0), (0.25, 0.36), (0.6, 0.6), (1.0, 1.0)]);
        let stack = Stack::new(Curve::identity(), Curve::identity(), Curve::identity(), toe, 1.0);
        let dark = stack.apply(&grey(ops::srgb_decode_scalar(0.25)));
        assert!(dark.d[2] > dark.d[0] * 1.5, "the shadows did not go blue: {:?}", dark.d);
        assert!((dark.d[0] - dark.d[1]).abs() < 1e-6, "red and green must not have moved");
        let light = stack.apply(&grey(ops::srgb_decode_scalar(0.9)));
        // Measured where the file lives: a cast below one 8-bit step cannot be
        // written down, let alone seen.
        let cast = (ops::srgb_encode_scalar(light.d[2]) - ops::srgb_encode_scalar(light.d[0])).abs();
        assert!(cast < 1.0 / 255.0,
                "the highlights picked up {:.4} of a cast, over one code value", cast);
    }

    /// Strength has to reach every channel, and at zero there has to be
    /// nothing left to apply rather than a very slight grade.
    #[test]
    fn strength_fades_the_whole_stack() {
        let hard = Curve::new(&[(0.0, 0.0), (0.3, 0.5), (1.0, 1.0)]);
        let full = Stack::new(hard.clone(), hard.clone(), Curve::identity(), hard.clone(), 1.0);
        let none = Stack::new(hard.clone(), hard.clone(), Curve::identity(), hard, 0.0);
        assert!(!full.is_identity());
        assert!(none.is_identity(), "a grade at zero strength is still being applied");
        let img = grey(0.2);
        assert_eq!(none.apply(&img).d, img.d);
        assert!((full.apply(&img).d[0] - 0.2).abs() > 0.05, "full strength did nothing");
    }

    /// The cache key has to move when the picture would and hold still when it
    /// would not, or a graded frame is served from a render of an ungraded one.
    #[test]
    fn a_fingerprint_follows_the_picture() {
        let a = Stack::from_json(Some(&serde_json::json!({"rgb": [[0, 0], [0.4, 0.5], [1, 1]]})));
        let b = Stack::from_json(Some(&serde_json::json!({"rgb": [[0, 0], [0.4, 0.5], [1, 1]]})));
        let c = Stack::from_json(Some(&serde_json::json!({"rgb": [[0, 0], [0.4, 0.51], [1, 1]]})));
        assert_eq!(a.fingerprint(), b.fingerprint(), "the same grade keyed two ways");
        assert_ne!(a.fingerprint(), c.fingerprint(), "a changed grade kept its key");
        assert_ne!(a.fingerprint(), 0, "a real grade keyed as the identity");
    }

    /// Nothing asked for is the ordinary case: most settings carry no curves
    /// at all, and a grade that touches one channel leaves three empty.
    #[test]
    fn asking_for_nothing_gets_the_identity() {
        assert!(Stack::from_json(None).is_identity());
        assert!(Stack::from_json(Some(&serde_json::json!({}))).is_identity());
        assert!(Stack::from_json(Some(&serde_json::json!({"rgb": []}))).is_identity());
        assert!(Stack::from_json(Some(&serde_json::json!({"strength": 0.8}))).is_identity());
    }

    /// A handle that arrives malformed is dropped rather than taken as zero,
    /// and the count is what makes it visible.
    #[test]
    fn a_malformed_point_is_dropped_and_counted() {
        let s = Stack::from_json(Some(&serde_json::json!({
            "rgb": [[0, 0], [0.3], "nonsense", [0.6, 0.7], [1, 1]],
            "b": [[0, 0], [0.5, 0.6], [1, 1]],
        })));
        assert_eq!(s.counts(), [3, 0, 0, 3], "counts: {:?}", s.counts());
    }

    /// Two points at the same place are one point, and the count has to say so
    /// rather than promise a handle that is not there.
    #[test]
    fn coincident_points_count_once() {
        let c = Curve::new(&[(0.0, 0.0), (0.5, 0.4), (0.5, 0.45), (1.0, 1.0)]);
        assert_eq!(c.knots(), 3);
        assert_eq!(Curve::identity().knots(), 0, "the identity has no control points");
    }


    // ---------------------------------------------------------------------
    // composition and the region sliders
    // ---------------------------------------------------------------------

    /// Composing has to mean composing, and it has to be free when either half
    /// has nothing to say.
    #[test]
    fn composing_applies_one_curve_to_the_other() {
        let a = Curve::new(&[(0.0, 0.0), (0.5, 0.62), (1.0, 1.0)]);
        let b = Curve::new(&[(0.0, 0.0), (0.5, 0.38), (1.0, 1.0)]);
        let both = a.then(&b);
        for i in 0..=100 {
            let x = i as f32 / 100.0;
            assert!((both.at(x) - b.at(a.at(x))).abs() < 2e-3,
                    "at {x}: {} against {}", both.at(x), b.at(a.at(x)));
        }
        assert_eq!(a.then(&Curve::identity()), a, "the identity changed something");
        assert_eq!(Curve::identity().then(&b), b, "the identity changed something");
        assert!(rises(&both), "two rising curves composed to a falling one");
    }

    /// Each slider has to move the part of the range it is named after and
    /// leave the others roughly where they were, or the four are one control
    /// with a confusing interface.
    #[test]
    fn each_region_moves_its_own_quarter() {
        let centres = [0.125f32, 0.375, 0.625, 0.875];
        for (i, centre) in centres.iter().enumerate() {
            let mut w = [0.0f32; 4];
            w[i] = 1.0;
            let c = parametric(w[0], w[1], w[2], w[3]);
            assert!(c.at(*centre) - centre > 0.1,
                    "region {i} barely moved its own centre: {}", c.at(*centre));
            // The far end of the range is not its business.
            let far = centres[if i < 2 { 3 } else { 0 }];
            assert!((c.at(far) - far).abs() < 0.045,
                    "region {i} moved {far} by {}", c.at(far) - far);
            assert!(rises(&c), "region {i} folded the curve");
        }
    }

    /// The ends belong to the point curve.
    #[test]
    fn the_region_sliders_leave_the_ends_alone() {
        let c = parametric(1.0, 1.0, -1.0, -1.0);
        assert!((c.at(0.0)).abs() < 1e-4, "black moved to {}", c.at(0.0));
        assert!((c.at(1.0) - 1.0).abs() < 1e-4, "white moved to {}", c.at(1.0));
        assert!(parametric(0.0, 0.0, 0.0, 0.0).is_identity(), "flat sliders are not the identity");
        // Even pushed every way at once it stays a curve, which is what lets
        // the four be moved without a combined guard on top.
        for w in [[1.0f32, -1.0, 1.0, -1.0], [-1.0, 1.0, -1.0, 1.0], [1.0; 4], [-1.0; 4]] {
            assert!(rises(&parametric(w[0], w[1], w[2], w[3])), "{w:?} folded");
        }
    }

    /// The two curves have to reach the pipeline together, and the points have
    /// to be the last word.
    #[test]
    fn regions_and_points_both_apply() {
        let json = serde_json::json!({
            "regions": {"shadows": 0.8},
            "rgb": [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]],
        });
        let s = Stack::from_json(Some(&json));
        assert!(!s.is_identity(), "the region slider never arrived");
        let only_regions = Stack::from_json(Some(&serde_json::json!({
            "regions": {"shadows": 0.8}})));
        assert_eq!(s.composite().table(), only_regions.composite().table(),
                   "a straight point curve changed what the sliders did");
    }

}

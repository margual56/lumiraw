//! Colour grading wheels: a colour pushed into the shadows, another into the
//! midtones, a third into the highlights, and one over everything, each with a
//! lightness of its own. What the mixer and the curves cannot do between them
//! without a dozen points: teal shadows under warm highlights, say.
//!
//! Every push is an offset in Oklab's a/b plane, so a wheel's hue is an Oklab
//! hue angle and the page draws its wheels in the same space; the colour under
//! the pointer is the colour that goes in.

use kit::{Image, Plane};
use serde_json::{json, Value};

/// The furthest a wheel at full amount moves a colour, in Oklab chroma.
pub const MAX_PUSH: f32 = 0.06;

/// How far a wheel's lightness slider, all the way over, moves the lightness
/// in the middle of its zone. Black and white themselves never move: a lifted
/// black or a dimmed white is a curve's job, and the wheels' colour is held
/// off black for the same reason (see `reach`).
pub const LUM_RANGE: f32 = 0.10;

/// How finely the lightness change is tabulated over 0..1.
const TONE_STEPS: usize = 1024;

/// One wheel: where the point is (an Oklab hue in degrees and how far out,
/// 0..1) and the slider under it (-1..1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    pub hue: f32,
    pub amount: f32,
    pub lum: f32,
}

impl Wheel {
    fn is_identity(&self) -> bool {
        self.amount.abs() < 1e-4 && self.lum.abs() < 1e-4
    }

    /// The a/b offset this wheel stands for at full weight.
    fn push(&self) -> (f32, f32) {
        let (sin, cos) = self.hue.to_radians().sin_cos();
        (cos * self.amount * MAX_PUSH, sin * self.amount * MAX_PUSH)
    }

    /// The wheel that stands for an a/b offset and a lightness change.
    fn from_push(a: f32, b: f32, dl: f32) -> Wheel {
        let amount = (a.hypot(b) / MAX_PUSH).min(1.0);
        let hue = if amount < 1e-4 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
        Wheel { hue, amount, lum: (dl / LUM_RANGE).clamp(-1.0, 1.0) }
    }

    fn from_json(v: Option<&Value>) -> Wheel {
        let Some(v) = v else { return Wheel::default() };
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        Wheel {
            hue: f("hue").rem_euclid(360.0),
            amount: f("amount").clamp(0.0, 1.0),
            lum: f("lum").clamp(-1.0, 1.0),
        }
    }

    fn to_json(self) -> Value {
        let r = |v: f32, d: f64| (v as f64 * d).round() / d;
        json!({"hue": r(self.hue, 10.0), "amount": r(self.amount, 1000.0),
               "lum": r(self.lum, 1000.0)})
    }
}

/// The four wheels and where the zones meet.
#[derive(Clone, Debug, PartialEq)]
pub struct Wheels {
    pub shadows: Wheel,
    pub midtones: Wheel,
    pub highlights: Wheel,
    pub global: Wheel,
    /// Where shadows give way to highlights, -1..1: up gives the highlights
    /// more of the frame.
    pub balance: f32,
    /// How far the zones overlap, 0..1; a half is the tone zones' own split.
    pub blending: f32,
}

impl Default for Wheels {
    fn default() -> Self {
        Wheels { shadows: Wheel::default(), midtones: Wheel::default(),
                 highlights: Wheel::default(), global: Wheel::default(),
                 balance: 0.0, blending: 0.5 }
    }
}

impl Wheels {
    pub fn is_identity(&self) -> bool {
        [self.shadows, self.midtones, self.highlights, self.global].iter().all(Wheel::is_identity)
    }

    pub fn from_json(v: Option<&Value>) -> Wheels {
        let Some(v) = v else { return Wheels::default() };
        Wheels {
            shadows: Wheel::from_json(v.get("shadows")),
            midtones: Wheel::from_json(v.get("midtones")),
            highlights: Wheel::from_json(v.get("highlights")),
            global: Wheel::from_json(v.get("global")),
            balance: v.get("balance").and_then(|x| x.as_f64()).unwrap_or(0.0).clamp(-1.0, 1.0) as f32,
            blending: v.get("blending").and_then(|x| x.as_f64()).unwrap_or(0.5).clamp(0.0, 1.0) as f32,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({"shadows": self.shadows.to_json(), "midtones": self.midtones.to_json(),
               "highlights": self.highlights.to_json(), "global": self.global.to_json(),
               "balance": self.balance, "blending": self.blending})
    }

    /// The lightness where shadows give way to highlights.
    fn pivot(&self) -> f32 {
        0.5 - 0.25 * self.balance
    }

    /// How much of a display lightness each zone owns: shadows, midtones,
    /// highlights, summing to one.
    #[inline]
    fn zones(&self, l: f32) -> (f32, f32, f32) {
        let pivot = self.pivot();
        let width = 0.2 + 0.6 * self.blending;
        let shadow = 1.0 - kit::smoothstep(pivot - width, pivot, l);
        let highlight = kit::smoothstep(pivot, pivot + width, l);
        (shadow, 1.0 - shadow - highlight, highlight)
    }
}

/// How each slider's lightness change is spread over a lightness `u` that has
/// been warped so the balance's pivot sits at a half: shadows, midtones,
/// highlights and everything, each zero at black and at white and peaking at
/// one in the middle of its zone (a third, a half, two thirds; the whole
/// range's middle for everything). Smooth all the way, so there is no step
/// where one zone gives way to the next, however little they blend.
#[inline]
fn tone_shapes(u: f32) -> [f32; 4] {
    let v = 1.0 - u;
    [6.75 * u * v * v, 4.0 * u * v, 6.75 * u * u * v, 4.0 * u * v]
}

/// The wheels' lightness as a table over 0..1, or `None` when the sliders are
/// all at zero. The table never runs downhill, so no combination of sliders
/// can turn a tone darker than the one under it.
fn tone_table(w: &Wheels) -> Option<Vec<f32>> {
    let lums = [w.shadows.lum, w.midtones.lum, w.highlights.lum, w.global.lum];
    if lums.iter().all(|v| v.abs() < 1e-4) {
        return None;
    }
    // u = l^k puts the pivot at a half.
    let k = 0.5f32.ln() / w.pivot().clamp(0.05, 0.95).ln();
    let mut floor = 0.0f32;
    Some((0..=TONE_STEPS).map(|i| {
        let u = (i as f32 / TONE_STEPS as f32).powf(k);
        let d: f32 = tone_shapes(u).iter().zip(lums).map(|(s, v)| s * v).sum();
        floor = floor.max((u + d * LUM_RANGE).clamp(0.0, 1.0).powf(1.0 / k));
        floor
    }).collect())
}

/// A lightness through `tone_table`'s table; anything outside 0..1 is left
/// where it is, as black and white are.
#[inline]
fn tone(table: &[f32], l: f32) -> f32 {
    if !(l > 0.0 && l < 1.0) {
        return l;
    }
    let x = l * TONE_STEPS as f32;
    let i = (x as usize).min(TONE_STEPS - 1);
    let t = x - i as f32;
    table[i] + (table[i + 1] - table[i]) * t
}

/// How much a colour is pushed at a lightness, before the zones: black stays
/// black, since a tint in the deepest shadow reads as a colour cast in the
/// sensor rather than as a grade.
#[inline]
fn reach(l: f32) -> f32 {
    kit::smoothstep(0.0, 0.15, l)
}

/// The wheels, on a display-linear picture. `protect` is, per pixel's hue and
/// chroma, how much of the push to hold back (skin, see `grade::Skin`).
pub fn apply(img: &Image, w: &Wheels, protect: impl Fn(f32, f32) -> f32) -> Image {
    if w.is_identity() {
        return img.clone();
    }
    let pushes = [w.shadows.push(), w.midtones.push(), w.highlights.push()];
    let global = w.global.push();
    let table = tone_table(w);
    let mut l = Plane::new(img.w, img.h);
    let mut pa = Plane::new(img.w, img.h);
    let mut pb = Plane::new(img.w, img.h);
    for (i, px) in img.px().iter().enumerate() {
        let (ll, a, b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
        let (s, m, h) = w.zones(ll);
        let weights = [s, m, h];
        let mut da = global.0;
        let mut db = global.1;
        for z in 0..3 {
            da += pushes[z].0 * weights[z];
            db += pushes[z].1 * weights[z];
        }
        let held = 1.0 - protect(b.atan2(a), a.hypot(b));
        let k = reach(ll) * held;
        l.d[i] = match &table {
            Some(t) => tone(t, ll),
            None => ll,
        };
        pa.d[i] = a + da * k;
        pb.d[i] = b + db * k;
    }
    kit::to_gamut(&l, &pa, &pb)
}

/// What a picture looks like zone by zone, with the zones split the default
/// way: the mean a, b and lightness under each zone's weight, how much weight
/// each zone has, and `mix[z][k]`, how much a push on zone `k` moves zone `z`'s
/// mean (the zones overlap, so a shadow push reaches the midtones' mean too).
struct Zones {
    mean: [(f32, f32, f32); 3],
    weight: [f32; 3],
    mix: [[f32; 3]; 3],
}

fn zones_of(img: &Image) -> Zones {
    let split = Wheels::default();
    let mut sums = [[0f64; 4]; 3];
    let mut mix = [[0f64; 3]; 3];
    for px in img.px() {
        let (l, a, b) = kit::rgb_to_oklab_px(px[0], px[1], px[2]);
        if !(l > 0.0) {
            continue;
        }
        let (s, m, h) = split.zones(l);
        let r = reach(l) as f64;
        let w = [s as f64 * r, m as f64 * r, h as f64 * r];
        for z in 0..3 {
            sums[z][0] += a as f64 * w[z];
            sums[z][1] += b as f64 * w[z];
            sums[z][2] += l as f64 * w[z];
            sums[z][3] += w[z];
            for k in 0..3 {
                mix[z][k] += w[z] * w[k];
            }
        }
    }
    let n = |z: usize| sums[z][3].max(1e-9);
    Zones {
        mean: std::array::from_fn(|z| ((sums[z][0] / n(z)) as f32, (sums[z][1] / n(z)) as f32,
                                       (sums[z][2] / n(z)) as f32)),
        weight: std::array::from_fn(|z| sums[z][3] as f32),
        mix: std::array::from_fn(|z| std::array::from_fn(|k| (mix[z][k] / n(z)) as f32)),
    }
}

/// `x` with `m x = y`, by Cramer's rule; a zone too thin to solve for comes
/// back zero.
fn solve(m: [[f32; 3]; 3], y: [f32; 3]) -> [f32; 3] {
    let det = |m: [[f32; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-6 {
        return [0.0; 3];
    }
    std::array::from_fn(|k| {
        let mut mk = m;
        for z in 0..3 {
            mk[z][k] = y[z];
        }
        det(mk) / d
    })
}

/// The wheels that would move `current`'s colour toward `reference`'s, zone by
/// zone: the shadows toward the reference's shadows, and so on. Each zone's
/// mean colour is matched, not its spread, which is what a wheel can do. A zone
/// one of the two pictures barely has is left alone.
pub fn fit(current: &Image, reference: &Image) -> Wheels {
    let (cur, refr) = (zones_of(current), zones_of(reference));
    let (nc, nr) = (current.px().len().max(1) as f32, reference.px().len().max(1) as f32);
    // Below a twentieth of either frame, a zone's mean is a few pixels'.
    let trust: [f32; 3] = std::array::from_fn(|z| {
        kit::smoothstep(0.02, 0.06, (cur.weight[z] / nc).min(refr.weight[z] / nr))
    });
    let gap = |f: fn(&(f32, f32, f32)) -> f32| -> [f32; 3] {
        std::array::from_fn(|z| f(&refr.mean[z]) - f(&cur.mean[z]))
    };
    let a = solve(cur.mix, gap(|m| m.0));
    let b = solve(cur.mix, gap(|m| m.1));
    // Half of the lightness gap only: how light a zone is depends on what is
    // in the picture far more than its colour does, and two unrelated scenes
    // matched all the way come out washed or murky. The mix is left out, since
    // lightness moves without the reach that holds colour off black.
    let l = gap(|m| m.2).map(|d| d * 0.5);
    let wheel = |z: usize| Wheel::from_push(a[z] * trust[z], b[z] * trust[z], l[z] * trust[z]);
    Wheels { shadows: wheel(0), midtones: wheel(1), highlights: wheel(2), ..Wheels::default() }
}

/// Two sets of wheels as one, zone by zone, the offsets adding; the balance
/// and blending are `a`'s.
pub fn add(a: &Wheels, b: &Wheels) -> Wheels {
    let one = |x: Wheel, y: Wheel| {
        let (xa, xb) = x.push();
        let (ya, yb) = y.push();
        Wheel::from_push(xa + ya, xb + yb, (x.lum + y.lum) * LUM_RANGE)
    };
    Wheels {
        shadows: one(a.shadows, b.shadows),
        midtones: one(a.midtones, b.midtones),
        highlights: one(a.highlights, b.highlights),
        global: one(a.global, b.global),
        ..a.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(level: f32) -> Image {
        Image::filled(4, 4, [level; 3])
    }

    fn lab(img: &Image) -> (f32, f32, f32) {
        kit::rgb_to_oklab_px(img.d[0], img.d[1], img.d[2])
    }

    #[test]
    fn untouched_wheels_change_nothing() {
        let src = Image::filled(3, 3, [0.4, 0.2, 0.1]);
        assert_eq!(apply(&src, &Wheels::default(), |_, _| 0.0).d, src.d);
    }

    /// The shadows wheel tints a dark grey and leaves a bright one alone, and
    /// the highlights wheel the other way round.
    #[test]
    fn each_wheel_keeps_to_its_zone() {
        let mut w = Wheels::default();
        w.shadows = Wheel { hue: 200.0, amount: 1.0, lum: 0.0 };
        w.highlights = Wheel { hue: 60.0, amount: 1.0, lum: 0.0 };
        let dark = lab(&apply(&grey(0.008), &w, |_, _| 0.0));
        let bright = lab(&apply(&grey(0.8), &w, |_, _| 0.0));
        // Shadows went toward 200 degrees (cyan-blue: a below zero), highlights
        // toward 60 (yellow-orange: b above zero).
        assert!(dark.1 < -0.02, "the shadows were not tinted: {dark:?}");
        assert!(bright.2 > 0.02, "the highlights were not tinted: {bright:?}");
        assert!(dark.2.abs() < bright.2 * 0.5, "the highlight colour reached the shadows");
    }

    #[test]
    fn black_stays_black() {
        let mut w = Wheels::default();
        w.global = Wheel { hue: 30.0, amount: 1.0, lum: 0.0 };
        let out = apply(&grey(0.0), &w, |_, _| 0.0);
        assert!(out.d.iter().all(|v| v.abs() < 1e-4), "{:?}", &out.d[..3]);
    }

    #[test]
    fn protection_holds_the_push_back() {
        let mut w = Wheels::default();
        w.global = Wheel { hue: 250.0, amount: 1.0, lum: 0.0 };
        let src = grey(0.4);
        let free = lab(&apply(&src, &w, |_, _| 0.0));
        let held = lab(&apply(&src, &w, |_, _| 1.0));
        assert!(free.1.hypot(free.2) > 0.03);
        assert!(held.1.hypot(held.2) < 1e-3);
    }

    #[test]
    fn the_zones_always_add_up_to_one() {
        for balance in [-1.0, 0.0, 0.7] {
            for blending in [0.0, 0.5, 1.0] {
                let w = Wheels { balance, blending, ..Wheels::default() };
                for i in 0..=20 {
                    let (s, m, h) = w.zones(i as f32 / 20.0);
                    assert!((s + m + h - 1.0).abs() < 1e-5 && m >= -1e-5);
                }
            }
        }
    }

    /// A reference that is the same picture with a tint in it gives back a
    /// grade that, applied, comes close to that tint.
    #[test]
    fn a_fit_finds_a_tint_it_is_shown() {
        let mut src = Image::new(32, 32);
        for (i, px) in src.px_mut().iter_mut().enumerate() {
            // Even steps of display lightness, so each zone has its share.
            let v = ((i % 32) as f32 / 31.0).powf(2.4);
            *px = [v * 0.9, v * 0.85, v * 0.8];
        }
        let mut tint = Wheels::default();
        tint.shadows = Wheel { hue: 210.0, amount: 0.6, lum: 0.0 };
        tint.highlights = Wheel { hue: 70.0, amount: 0.5, lum: 0.0 };
        let reference = apply(&src, &tint, |_, _| 0.0);
        let fitted = fit(&src, &reference);
        let again = apply(&src, &fitted, |_, _| 0.0);
        let (r, g) = (zones_of(&reference).mean, zones_of(&again).mean);
        for z in [0, 2] {
            assert!((r[z].0 - g[z].0).abs() < 0.006 && (r[z].1 - g[z].1).abs() < 0.006,
                    "zone {z}: wanted {:?}, got {:?}", r[z], g[z]);
        }
        assert!((fitted.shadows.hue - 210.0).abs() < 25.0, "{fitted:?}");
        assert!((fitted.highlights.hue - 70.0).abs() < 25.0, "{fitted:?}");
    }

    #[test]
    fn adding_wheels_adds_their_pushes() {
        let mut a = Wheels::default();
        a.midtones = Wheel { hue: 0.0, amount: 0.3, lum: 0.2 };
        let mut b = Wheels::default();
        b.midtones = Wheel { hue: 0.0, amount: 0.2, lum: -0.1 };
        let sum = add(&a, &b);
        assert!((sum.midtones.amount - 0.5).abs() < 1e-4 && (sum.midtones.lum - 0.1).abs() < 1e-4);
    }

    fn lightness_of(w: &Wheels, level: f32) -> f32 {
        lab(&apply(&grey(level), w, |_, _| 0.0)).0
    }

    /// Black and white stay where they are, however the sliders are set.
    #[test]
    fn lightness_leaves_black_and_white_alone() {
        for lum in [-1.0, 1.0] {
            for zone in 0..4 {
                let mut w = Wheels::default();
                match zone {
                    0 => w.shadows.lum = lum,
                    1 => w.midtones.lum = lum,
                    2 => w.highlights.lum = lum,
                    _ => w.global.lum = lum,
                }
                assert!(lightness_of(&w, 0.0).abs() < 1e-4, "zone {zone} at {lum} moved black");
                assert!((lightness_of(&w, 1.0) - 1.0).abs() < 1e-3, "zone {zone} at {lum} moved white");
            }
        }
    }

    /// No setting of the sliders, however extreme, makes a darker tone come
    /// out lighter than a brighter one.
    #[test]
    fn lightness_never_runs_downhill() {
        let ends = [-1.0, 0.0, 1.0];
        for &s in &ends { for &m in &ends { for &h in &ends { for &g in &ends {
            for balance in [-1.0, 0.0, 1.0] {
                let mut w = Wheels { balance, ..Wheels::default() };
                w.shadows.lum = s;
                w.midtones.lum = m;
                w.highlights.lum = h;
                w.global.lum = g;
                let t = tone_table(&w).unwrap_or_else(|| vec![0.0; 2]);
                assert!(t.windows(2).all(|p| p[1] >= p[0]), "{s} {m} {h} {g} {balance}");
            }
        }}}}
    }

    /// The shadows slider moves the shadows and barely the highlights, and the
    /// highlights slider the other way round.
    #[test]
    fn each_lightness_keeps_to_its_zone() {
        // Display-linear greys at about a quarter and three quarters of
        // Oklab lightness.
        let (dark, bright) = (0.016, 0.42);
        let base = Wheels::default();
        let mut up = Wheels::default();
        up.shadows.lum = 1.0;
        let lift = lightness_of(&up, dark) - lightness_of(&base, dark);
        let spill = lightness_of(&up, bright) - lightness_of(&base, bright);
        assert!(lift > 0.07, "the shadows barely moved: {lift}");
        assert!(spill < lift * 0.4, "the shadows slider reached the highlights: {spill}");
        let mut down = Wheels::default();
        down.highlights.lum = -1.0;
        let drop = lightness_of(&base, bright) - lightness_of(&down, bright);
        let spill = lightness_of(&base, dark) - lightness_of(&down, dark);
        assert!(drop > 0.07, "the highlights barely moved: {drop}");
        assert!(spill < drop * 0.4, "the highlights slider reached the shadows: {spill}");
    }

    #[test]
    fn json_round_trips() {
        let mut w = Wheels::default();
        w.highlights = Wheel { hue: 45.5, amount: 0.25, lum: -0.5 };
        w.balance = 0.3;
        assert_eq!(Wheels::from_json(Some(&w.to_json())), w);
    }
}

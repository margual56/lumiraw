//! Turn each named look's 3D table into the controls that come closest to it:
//! the curves, three wheels, the mixer and, for black and white, monochrome. A
//! look is then nothing but a setting of controls the photographer can see and
//! move on from; the tables themselves never reach the app. Run when a look is
//! added or changed; not part of any build.
//!
//!     python tools/looks.py HALDCLUT_DIR        # the tables, into tools/looks
//!     cargo run --release --manifest-path core/Cargo.toml --bin fitlooks -- \
//!         tools/looks > web/src/lib/looks.json
//!
//! The fit runs the controls exactly as `develop::finish` does (mixer, then
//! monochrome, then wheels, then curves) over a spread of colours, weighted
//! toward the low chroma most of any photograph is, and moves one control at
//! a time while that brings the result nearer the table's.

use kit::Image;
use lumiraw_core::{curve, effects, looks, lut::Lut, mixer::Mixer, wheels};
use serde_json::{json, Value};

/// Where the rgb curve's points sit along its input.
const KNOTS: [f32; 7] = [0.0, 0.1, 0.25, 0.45, 0.65, 0.85, 1.0];
const N: usize = KNOTS.len();

/// Where the red, green and blue curves' points sit. They are for what the
/// wheels cannot reach, such as a tint right up to white, so they are fewer
/// and cost more to move.
const CH_KNOTS: [f32; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];
const CN: usize = CH_KNOTS.len();

/// What the fit moves: the rgb curve's outputs, the red, green and blue
/// curves', the three zone wheels as Oklab a/b pushes in units of
/// `wheels::MAX_PUSH` and their lightness, and the mixer.
#[derive(Clone)]
struct Params {
    curve: [f32; N],
    chan: [[f32; CN]; 3],
    push: [[f32; 2]; 3],
    lum: [f32; 3],
    mixer: Mixer,
}

impl Params {
    fn identity() -> Params {
        Params { curve: KNOTS, chan: [CH_KNOTS; 3], push: [[0.0; 2]; 3], lum: [0.0; 3], mixer: Mixer::default() }
    }

    fn len() -> usize {
        N + 3 * CN + 6 + 3 + 24
    }

    fn get(&mut self, i: usize) -> &mut f32 {
        match i {
            i if i < N => &mut self.curve[i],
            i if i < N + 3 * CN => &mut self.chan[(i - N) / CN][(i - N) % CN],
            i if i < N + 3 * CN + 6 => {
                let j = i - N - 3 * CN;
                &mut self.push[j / 2][j % 2]
            }
            i if i < N + 3 * CN + 9 => &mut self.lum[i - N - 3 * CN - 6],
            _ => {
                let j = i - N - 3 * CN - 9;
                let band = j % 8;
                match j / 8 {
                    0 => &mut self.mixer.hue[band],
                    1 => &mut self.mixer.sat[band],
                    _ => &mut self.mixer.lum[band],
                }
            }
        }
    }

    /// Each control's own range.
    fn bounds(i: usize) -> (f32, f32) {
        if i < N + 3 * CN { (0.0, 1.0) } else { (-1.0, 1.0) }
    }

    fn wheel(&self, z: usize) -> wheels::Wheel {
        let [a, b] = self.push[z];
        let amount = a.hypot(b).min(1.0);
        let hue = if amount < 1e-4 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
        wheels::Wheel { hue, amount, lum: self.lum[z] }
    }

    fn wheels(&self) -> wheels::Wheels {
        wheels::Wheels { shadows: self.wheel(0), midtones: self.wheel(1),
                         highlights: self.wheel(2), ..wheels::Wheels::default() }
    }

    fn curves_json(&self) -> Value {
        let r = |v: f32| (v as f64 * 1000.0).round() / 1000.0;
        let moved = self.curve.iter().zip(KNOTS).any(|(y, x)| (y - x).abs() > 1e-3);
        let points: Vec<Value> = if moved {
            KNOTS.iter().zip(self.curve).map(|(x, y)| json!([r(*x), r(y)])).collect()
        } else {
            Vec::new()
        };
        let chan = |c: usize| -> Vec<Value> {
            if self.chan[c].iter().zip(CH_KNOTS).all(|(y, x)| (y - x).abs() <= 1e-3) {
                return Vec::new();
            }
            CH_KNOTS.iter().zip(self.chan[c]).map(|(x, y)| json!([r(*x), r(y)])).collect()
        };
        json!({"strength": 1, "rgb": points, "r": chan(0), "g": chan(1), "b": chan(2)})
    }

    fn mixer_json(&self) -> Value {
        let r = |v: f32| (v as f64 * 100.0).round() / 100.0;
        let mut out = serde_json::Map::new();
        // Only the bands that move, as the page keeps them.
        for (i, name) in lumiraw_core::mixer::BANDS.iter().enumerate() {
            let band = [self.mixer.hue[i], self.mixer.sat[i], self.mixer.lum[i]].map(r);
            if band.iter().any(|v| *v != 0.0) {
                out.insert(name.to_string(), json!({"h": band[0], "s": band[1], "l": band[2]}));
            }
        }
        Value::Object(out)
    }

    /// The controls' own rounding, so what is fitted is what is stored.
    fn rounded(&self) -> Params {
        let settings = self.settings(0.0);
        Params {
            curve: self.curve.map(|v| (v * 1000.0).round() / 1000.0),
            chan: self.chan.map(|c| c.map(|v| (v * 1000.0).round() / 1000.0)),
            push: std::array::from_fn(|z| {
                let w = wheels::Wheels::from_json(settings.get("wheels"));
                let w = [w.shadows, w.midtones, w.highlights][z];
                let (s, c) = w.hue.to_radians().sin_cos();
                [c * w.amount, s * w.amount]
            }),
            lum: std::array::from_fn(|z| {
                let w = wheels::Wheels::from_json(settings.get("wheels"));
                [w.shadows, w.midtones, w.highlights][z].lum
            }),
            mixer: Mixer::from_json(Some(&self.mixer_json())),
        }
    }

    /// The settings these controls are, as the page holds them.
    fn settings(&self, monochrome: f32) -> Value {
        json!({"curves": self.curves_json(), "wheels": self.wheels().to_json(),
               "mixer": self.mixer_json(), "monochrome": monochrome})
    }
}

/// The colours the fit is judged on, display-linear, and how much each counts.
fn samples() -> (Image, Vec<f32>) {
    let steps = 11;
    let mut px = Vec::new();
    for r in 0..steps {
        for g in 0..steps {
            for b in 0..steps {
                let enc = [r, g, b].map(|v| v as f32 / (steps - 1) as f32);
                px.push(enc.map(kit::srgb_decode_scalar));
            }
        }
    }
    // Greys twice over, finely: every photograph is mostly near them.
    for i in 0..=64 {
        let v = kit::srgb_decode_scalar(i as f32 / 64.0);
        px.push([v; 3]);
        px.push([v; 3]);
    }
    // And the faintly tinted colours most of a photograph actually is: off
    // whites, skin in shade, overcast sky, stone.
    for l in 0..11 {
        for h in 0..12 {
            for c in [0.015f32, 0.03, 0.05] {
                let lightness = 0.2 + 0.075 * l as f32;
                let (sin, cos) = (h as f32 * std::f32::consts::TAU / 12.0).sin_cos();
                let (r, g, b) = kit::oklab_to_rgb_px(lightness, cos * c, sin * c);
                if [r, g, b].iter().all(|v| (0.0..=1.0).contains(v)) {
                    px.push([r, g, b]);
                }
            }
        }
    }
    let mut img = Image::new(px.len(), 1);
    img.px_mut().copy_from_slice(&px);
    let weight = img.px().iter().map(|p| {
        let (_, a, b) = kit::rgb_to_oklab_px(p[0], p[1], p[2]);
        // Saturated primaries matter, but far less than the colours of
        // skin, foliage, sky and stone.
        0.4 + (-(a.hypot(b)) / 0.09).exp()
    }).collect();
    (img, weight)
}

/// The controls, run over the samples as `develop::finish` runs them.
fn run(src: &Image, p: &Params, monochrome: f32) -> Image {
    let mut img = lumiraw_core::mixer::apply(src, &p.mixer);
    effects::monochrome(&mut img, monochrome);
    let img = wheels::apply(&img, &p.wheels(), |_, _| 0.0);
    curve::Stack::from_json(Some(&p.curves_json())).apply(&img)
}

/// How far two pictures are apart, pixel by pixel in Oklab, weighted.
fn distance(a: &Image, b: &Image, weight: &[f32]) -> f32 {
    let mut sum = 0.0;
    let mut total = 0.0;
    for ((x, y), w) in a.px().iter().zip(b.px()).zip(weight) {
        let p = kit::rgb_to_oklab_px(x[0], x[1], x[2]);
        let q = kit::rgb_to_oklab_px(y[0], y[1], y[2]);
        // Lightness errors read about as strongly as colour ones.
        let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2) + (p.2 - q.2).powi(2)).sqrt();
        sum += d * w;
        total += w;
    }
    sum / total
}

fn fit(table: &Lut, monochrome: f32) -> (Params, f32, f32) {
    let (src, weight) = samples();
    let target = table.apply(&src, 1.0);
    let mut p = Params::identity();
    let start = distance(&run(&src, &p, monochrome), &target, &weight);
    let mut best = start;
    let mut step = vec![0.1f32; Params::len()];
    // Small controls cost a little, so a control the table does not need
    // stays at zero instead of wandering to cancel another one out.
    let cost = |p: &Params, err: f32| -> f32 {
        let mut c = 0.0;
        for z in 0..3 {
            c += p.push[z][0].abs() + p.push[z][1].abs() + p.lum[z].abs();
        }
        for ch in &p.chan {
            for (y, x) in ch.iter().zip(CH_KNOTS) {
                c += (y - x).abs() * 4.0;
            }
        }
        for i in 0..8 {
            c += p.mixer.hue[i].abs() + p.mixer.sat[i].abs() + p.mixer.lum[i].abs();
        }
        err + c * 0.00004
    };
    let mut score = cost(&p, best);
    for _round in 0..400 {
        let mut moved = false;
        for i in 0..Params::len() {
            let (lo, hi) = Params::bounds(i);
            for dir in [1.0f32, -1.0] {
                let mut q = p.clone();
                let v = q.get(i);
                *v = (*v + dir * step[i]).clamp(lo, hi);
                let err = distance(&run(&src, &q, monochrome), &target, &weight);
                let s = cost(&q, err);
                if s < score {
                    p = q;
                    score = s;
                    best = err;
                    step[i] *= 1.3;
                    moved = true;
                    break;
                }
            }
            step[i] = (step[i] * 0.7).max(0.002);
        }
        if !moved && step.iter().all(|s| *s <= 0.0021) {
            break;
        }
    }
    let p = p.rounded();
    let fitted = run(&src, &p, monochrome);
    let end = distance(&fitted, &target, &weight);
    let _ = best;
    // Where what is left over is: in the greys, or in the colours.
    let grey: Vec<f32> = src.px().iter().map(|q| {
        let (_, a, b) = kit::rgb_to_oklab_px(q[0], q[1], q[2]);
        if a.hypot(b) < 0.01 { 1.0 } else { 0.0 }
    }).collect();
    let low: Vec<f32> = src.px().iter().map(|q| {
        let (_, a, b) = kit::rgb_to_oklab_px(q[0], q[1], q[2]);
        let c = a.hypot(b);
        if c >= 0.01 && c < 0.08 { 1.0 } else { 0.0 }
    }).collect();
    eprintln!("    greys {:.4}, muted colours {:.4}", distance(&fitted, &target, &grey),
              distance(&fitted, &target, &low));
    (p, start, end)
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: fitlooks <dir with look-*.bin>");
    let mut out = serde_json::Map::new();
    for look in looks::LOOKS.iter().filter(|l| l.id != "none") {
        let path = format!("{dir}/look-{}.bin", look.id);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let table = Lut::from_bytes(look.label, &bytes).expect("a packed table");
        let monochrome = if look.id == "black-and-white" { 1.0 } else { 0.0 };
        let (p, before, after) = fit(&table, monochrome);
        eprintln!("{:<16} distance from the table: {:.4} untouched, {:.4} fitted",
                  look.id, before, after);
        out.insert(look.id.to_string(), p.settings(monochrome));
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(out)).unwrap());
}

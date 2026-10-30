//! Deep noise reduction on the raw mosaic, before anything else touches it:
//! PMRID (Wang et al., "Practical Deep Raw Image Denoising on Mobile Devices",
//! ECCV 2020), baked by `tools/bake-denoiser.mjs`.
//!
//! The network was trained on one sensor's noise at one ISO (its "anchor").
//! Any other noise is brought to that one first: sensor noise is
//! Poisson–Gaussian, variance `k·x + σ²`, and an affine change of the values
//! (the paper's k-sigma transform) turns one such noise into another. Here
//! `k` and `σ²` are measured on the frame itself (`estimate_noise`), since a
//! camera nobody has calibrated has to work too.
//!
//! Only 2 × 2 Bayer mosaics: the network sees R, G, G, B packed into four
//! half-size channels.

use crate::raw::CFA;
use critic::graph::Graph;
use kit::Plane;

/// The scale the network's training data was in: a 10-bit sensor's range
/// above black.
const V: f32 = 959.0;
/// The values the network takes, times the 0..1 range.
const INPUT_SCALE: f32 = 256.0;
/// The anchor's noise, in units of `V`: `k` and `σ²` at ISO 1600 on the
/// training sensor, from PMRID's `run_benchmark.py`.
const ANCHOR_K: f32 = 0.0005995267 * 1600.0 + 0.00868861;
const ANCHOR_VAR: f32 = 7.11772e-7 * 1600.0 * 1600.0 + 6.514934e-4 * 1600.0 + 0.11492713;
/// Packed pixels of context each tile carries on every side and throws away.
const MARGIN: usize = 16;

/// Poisson–Gaussian noise: at a value `x` (0..1 of the sensor's range), the
/// variance is `gain · x + floor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    pub gain: f32,
    pub floor: f32,
}

/// Where the red photosite of a 2 × 2 Bayer pattern sits, as (column, row)
/// within the pattern; `None` for anything else (X-Trans, say).
pub fn bayer_origin(cfa: &CFA) -> Option<(usize, usize)> {
    // A 2 × 2 pattern repeats every two photosites both ways.
    for y in 0..4 {
        for x in 0..4 {
            if cfa.color_at(y, x) != cfa.color_at(y % 2, x % 2) {
                return None;
            }
        }
    }
    for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let at = |dx: usize, dy: usize| cfa.color_at(oy + dy, ox + dx);
        if at(0, 0) == 0 && at(1, 0) == 1 && at(0, 1) == 1 && at(1, 1) == 2 {
            return Some((ox, oy));
        }
    }
    None
}

/// Four half-size planes R, G, G, B of the mosaic's area `(x0, y0, w, h)`,
/// starting at the first red photosite in it.
fn pack(cfa: &Plane, origin: (usize, usize), area: (usize, usize, usize, usize))
        -> (Vec<[f32; 4]>, usize, usize, usize, usize) {
    let (x0, y0, w, h) = area;
    // The first red inside the area, at absolute sensor positions.
    let sx = x0 + (origin.0 + 2 - x0 % 2) % 2;
    let sy = y0 + (origin.1 + 2 - y0 % 2) % 2;
    let pw = (x0 + w - sx) / 2;
    let ph = (y0 + h - sy) / 2;
    let mut out = Vec::with_capacity(pw * ph);
    for y in 0..ph {
        let r0 = (sy + 2 * y) * cfa.w + sx;
        let r1 = r0 + cfa.w;
        for x in 0..pw {
            out.push([cfa.d[r0 + 2 * x], cfa.d[r0 + 2 * x + 1], cfa.d[r1 + 2 * x], cfa.d[r1 + 2 * x + 1]]);
        }
    }
    (out, pw, ph, sx, sy)
}

/// The mosaic's noise, measured on itself: in small blocks of each packed
/// channel, the spread of what a block's neighbours do not explain, against
/// how bright it is. Texture only ever adds spread, so in each band of
/// brightness the quietest blocks are the noise, and a line through those is
/// the answer.
pub fn estimate_noise(packed: &[[f32; 4]], pw: usize, ph: usize) -> Noise {
    const BLOCK: usize = 8;
    const BANDS: usize = 24;
    let mut bands: Vec<Vec<f32>> = vec![Vec::new(); BANDS];
    for by in (1..ph.saturating_sub(BLOCK + 1)).step_by(BLOCK) {
        for bx in (1..pw.saturating_sub(BLOCK + 1)).step_by(BLOCK) {
            for c in 0..4 {
                let (mut sum, mut sq) = (0f64, 0f64);
                let mut mean = 0f64;
                for y in by..by + BLOCK {
                    for x in bx..bx + BLOCK {
                        let v = packed[y * pw + x][c];
                        let around = (packed[y * pw + x - 1][c] + packed[y * pw + x + 1][c]
                            + packed[(y - 1) * pw + x][c] + packed[(y + 1) * pw + x][c]) / 4.0;
                        let r = (v - around) as f64;
                        sum += r;
                        sq += r * r;
                        mean += v as f64;
                    }
                }
                let n = (BLOCK * BLOCK) as f64;
                let mean = (mean / n) as f32;
                // A residual against four neighbours has 1 + 1/4 of the noise's variance.
                let var = ((sq / n - (sum / n).powi(2)) / 1.25) as f32;
                if mean > 0.002 && mean < 0.9 {
                    let band = ((mean / 0.9) * BANDS as f32) as usize;
                    bands[band.min(BANDS - 1)].push(var);
                }
            }
        }
    }
    // The lower quartile of each band, then a weighted straight line. Even
    // pure noise measured on 64 photosites spreads, and its lower quartile
    // sits at about 0.86 of the true variance (a chi-squared's, 63 degrees).
    let points: Vec<(f32, f32, f32)> = bands.iter_mut().enumerate().filter(|(_, b)| b.len() >= 8)
        .map(|(i, b)| {
            b.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let low = b[b.len() / 4] / 0.86;
            ((i as f32 + 0.5) / BANDS as f32 * 0.9, low, (b.len() as f32).sqrt())
        })
        .collect();
    let floor_only = || Noise { gain: 1e-7, floor: points.first().map_or(1e-6, |p| p.1.max(1e-9)) };
    if points.len() < 2 {
        return floor_only();
    }
    let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0f32, 0f32, 0f32, 0f32, 0f32);
    for (x, y, w) in &points {
        sw += w;
        sx += w * x;
        sy += w * y;
        sxx += w * x * x;
        sxy += w * x * y;
    }
    let d = sw * sxx - sx * sx;
    if d.abs() < 1e-12 {
        return floor_only();
    }
    let gain = (sw * sxy - sx * sy) / d;
    let floor = (sy - gain * sx) / sw;
    Noise { gain: gain.max(1e-7), floor: floor.max(1e-9) }
}

/// The k-sigma transform toward the anchor, as a scale and an offset on
/// values in units of `V`.
fn toward_anchor(noise: Noise) -> (f32, f32) {
    let k = noise.gain * V;
    let var = noise.floor * V * V;
    let scale = ANCHOR_K / k;
    let offset = (var / (k * k) - ANCHOR_VAR / (ANCHOR_K * ANCHOR_K)) * ANCHOR_K;
    (scale, offset)
}

/// Denoise the area `(x0, y0, w, h)` of a normalised mosaic (black 0,
/// saturation 1, white balance not applied) in place. `progress` hears the
/// fraction of tiles done. Returns the noise it measured, or `None` when the
/// pattern is not a 2 × 2 Bayer one and nothing was done.
pub fn denoise(cfa: &mut Plane, pattern: &CFA, area: (usize, usize, usize, usize), net: &Graph,
               progress: &mut dyn FnMut(f32)) -> Option<Noise> {
    let origin = bayer_origin(pattern)?;
    let (mut packed, pw, ph, sx, sy) = pack(cfa, origin, area);
    if pw < 2 * MARGIN || ph < 2 * MARGIN {
        return None;
    }
    let noise = estimate_noise(&packed, pw, ph);
    let (scale, offset) = toward_anchor(noise);
    let to_net = |v: f32| ((v.clamp(0.0, 1.0) * V * scale + offset) / V) * INPUT_SCALE;
    let from_net = |v: f32| ((v / INPUT_SCALE * V - offset) / scale) / V;

    let (tile, _, _) = net.input_size();
    let step = tile - 2 * MARGIN;
    let reflect = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let mut i = i;
        if i < 0 { i = -i; }
        if i >= n { i = 2 * (n - 1) - i; }
        i.clamp(0, n - 1) as usize
    };
    let tiles_x = pw.div_ceil(step);
    let tiles_y = ph.div_ceil(step);
    let source = packed.clone();
    let mut input = vec![0f32; tile * tile * 4];
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let (ox, oy) = ((tx * step) as isize - MARGIN as isize, (ty * step) as isize - MARGIN as isize);
            for y in 0..tile {
                let syy = reflect(oy + y as isize, ph);
                for x in 0..tile {
                    let px = source[syy * pw + reflect(ox + x as isize, pw)];
                    let at = (y * tile + x) * 4;
                    for c in 0..4 {
                        input[at + c] = to_net(px[c]);
                    }
                }
            }
            let out = net.run(input.clone()).swap_remove(0);
            for y in MARGIN..tile - MARGIN {
                let py = oy + y as isize;
                if py < 0 || py >= ph as isize {
                    continue;
                }
                for x in MARGIN..tile - MARGIN {
                    let px = ox + x as isize;
                    if px < 0 || px >= pw as isize {
                        continue;
                    }
                    let at = (y * tile + x) * 4;
                    let dst = &mut packed[py as usize * pw + px as usize];
                    for c in 0..4 {
                        dst[c] = from_net(out[at + c]).max(0.0);
                    }
                }
            }
            progress((ty * tiles_x + tx + 1) as f32 / (tiles_x * tiles_y) as f32);
        }
    }
    for y in 0..ph {
        let r0 = (sy + 2 * y) * cfa.w + sx;
        let r1 = r0 + cfa.w;
        for x in 0..pw {
            let v = packed[y * pw + x];
            cfa.d[r0 + 2 * x] = v[0];
            cfa.d[r0 + 2 * x + 1] = v[1];
            cfa.d[r1 + 2 * x] = v[2];
            cfa.d[r1 + 2 * x + 1] = v[3];
        }
    }
    Some(noise)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny deterministic generator, so the noise is the same every run.
    struct Rng(u64);
    impl Rng {
        fn normal(&mut self) -> f32 {
            let mut u = || {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((self.0 >> 40) as f32 + 0.5) / (1u64 << 24) as f32
            };
            let (a, b) = (u(), u());
            (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos()
        }
    }

    #[test]
    fn the_red_photosite_is_found_in_every_bayer_order() {
        for (name, at) in [("RGGB", (0, 0)), ("GRBG", (1, 0)), ("GBRG", (0, 1)), ("BGGR", (1, 1))] {
            assert_eq!(bayer_origin(&CFA::new(name)), Some(at), "{name}");
        }
    }

    #[test]
    fn packing_starts_at_a_red_photosite_wherever_the_area_starts() {
        let mut cfa = Plane::new(8, 8);
        for (i, v) in cfa.d.iter_mut().enumerate() {
            *v = i as f32;
        }
        // RGGB, area from (1, 1): the first red inside is at (2, 2).
        let (p, pw, ph, sx, sy) = pack(&cfa, (0, 0), (1, 1, 7, 7));
        assert_eq!((pw, ph, sx, sy), (3, 3, 2, 2));
        assert_eq!(p[0], [18.0, 19.0, 26.0, 27.0]);
    }

    /// Noise made with a known gain and floor is measured back near enough,
    /// on a scene with some texture in it to be fooled by.
    #[test]
    fn noise_is_measured_on_the_frame_itself() {
        let (pw, ph) = (400, 300);
        let truth = Noise { gain: 4e-4, floor: 2e-6 };
        let mut rng = Rng(7);
        let packed: Vec<[f32; 4]> = (0..pw * ph).map(|i| {
            let (x, y) = ((i % pw) as f32, (i / pw) as f32);
            let clean = 0.02 + 0.6 * x / pw as f32 + 0.05 * (y * 0.3).sin().max(0.0) * (y < 40.0) as u8 as f32;
            std::array::from_fn(|_| clean + (truth.gain * clean + truth.floor).sqrt() * rng.normal())
        }).collect();
        let got = estimate_noise(&packed, pw, ph);
        assert!((got.gain / truth.gain - 1.0).abs() < 0.25, "gain {got:?}");
        assert!(got.floor < 2e-5, "floor {got:?}");
    }

    /// The committed network reads, and is laid out for the tile size this
    /// module cuts frames into.
    #[test]
    fn the_baked_network_is_one_tile_of_four_channels() {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),
                                          "/../web/src/lib/wasm/critic/denoiser.bin")).unwrap();
        let net = Graph::parse(&bytes).unwrap();
        let (h, w, c) = net.input_size();
        assert_eq!((h, w, c), (256, 256, 4));
        assert!(h > 2 * MARGIN);
    }

    /// Noise at the anchor's own level is left as it is by the transform.
    #[test]
    fn the_anchor_maps_to_itself() {
        let (scale, offset) = toward_anchor(Noise { gain: ANCHOR_K / V, floor: ANCHOR_VAR / (V * V) });
        assert!((scale - 1.0).abs() < 1e-5 && offset.abs() < 1e-3, "{scale} {offset}");
    }
}

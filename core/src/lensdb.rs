//! Lens corrections from the lensfun database, evaluated ourselves.

use kit::Image;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, Clone, Debug)]
pub struct CameraEntry {
    pub maker: String,
    pub model: String,
    #[serde(default)]
    pub mount: String,
    #[serde(default = "one")]
    pub crop: f32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct DistEntry {
    pub f: f32,
    pub m: String,
    #[serde(default)] pub a: f32,
    #[serde(default)] pub b: f32,
    #[serde(default)] pub c: f32,
    #[serde(default)] pub k1: f32,
    #[serde(default)] pub k2: f32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct TcaEntry {
    pub f: f32,
    pub m: String,
    #[serde(default)] pub br: f32,
    #[serde(default)] pub cr: f32,
    #[serde(default = "one")] pub vr: f32,
    #[serde(default)] pub bb: f32,
    #[serde(default)] pub cb: f32,
    #[serde(default = "one")] pub vb: f32,
    #[serde(default = "one")] pub kr: f32,
    #[serde(default = "one")] pub kb: f32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct VignEntry {
    pub f: f32,
    pub ap: f32,
    #[serde(default)] pub d: f32,
    #[serde(default)] pub k1: f32,
    #[serde(default)] pub k2: f32,
    #[serde(default)] pub k3: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LensEntry {
    pub maker: String,
    pub model: String,
    #[serde(default)]
    pub mount: String,
    #[serde(default = "one")]
    pub crop: f32,
    #[serde(default)]
    pub dist: Vec<DistEntry>,
    #[serde(default)]
    pub tca: Vec<TcaEntry>,
    #[serde(default)]
    pub vign: Vec<VignEntry>,
}

fn one() -> f32 {
    1.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct Database {
    #[serde(default)]
    pub cameras: Vec<CameraEntry>,
    #[serde(default)]
    pub lenses: Vec<LensEntry>,
    /// Absent for the whole database.
    #[serde(default)]
    pub mounts: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct LensMatch {
    pub camera: String,
    pub lens: String,
    pub crop: f32,
    pub focal: f32,
    pub aperture: f32,
    pub distance: f32,
    pub entry: LensEntry,
}

impl Database {
    pub fn parse(bytes: &[u8]) -> Result<Database, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    /// Take in another part of the database, one mount's lenses as a rule.
    pub fn extend(&mut self, other: Database) {
        self.cameras.extend(other.cameras);
        self.lenses.extend(other.lenses);
        if let Some(mounts) = &mut self.mounts {
            mounts.extend(other.mounts.unwrap_or_default());
        }
    }

    /// The mount whose lenses this photograph's lens has to be looked up among,
    /// when they have not been loaded.
    pub fn missing_mount(&self, make: &str, model: &str, lens: &str) -> Option<String> {
        let loaded = self.mounts.as_ref()?;
        let mount = self.find_camera(make, model)?.mount.to_lowercase();
        (!norm(lens).is_empty() && !loaded.contains(&mount)).then_some(mount)
    }

    /// The camera body on its own: worth doing even when the lens is unknown,
    /// because the body gives an authoritative crop factor and hence the pixel
    /// pitch the diffraction and noise priors are built on.
    pub fn find_camera(&self, make: &str, model: &str) -> Option<&CameraEntry> {
        let m = norm(model);
        if m.is_empty() {
            return None;
        }
        let maker = norm(make);
        self.cameras
            .iter()
            .find(|c| norm(&c.model) == m && (maker.is_empty() || norm(&c.maker) == maker))
            .or_else(|| self.cameras.iter().find(|c| norm(&c.model) == m))
    }

    pub fn find_lens(&self, make: &str, model: &str, lens: &str, focal: f32, aperture: f32,
                     distance: f32) -> Option<LensMatch> {
        let cam = self.find_camera(make, model)?;
        let want = norm(lens);
        if want.is_empty() {
            return None;
        }
        let mount = cam.mount.to_lowercase();
        // A part of the database without this mount cannot answer yet, and
        // must not answer from another mount's lenses in the meantime.
        if self.mounts.as_ref().is_some_and(|loaded| !loaded.contains(&mount)) {
            return None;
        }
        let mut candidates: Vec<&LensEntry> = self
            .lenses
            .iter()
            .filter(|l| mount.is_empty() || l.mount.to_lowercase() == mount)
            .collect();
        if candidates.is_empty() {
            candidates = self.lenses.iter().collect();
        }
        // Exact name first; then either name containing the other, which is
        // what catches "E 55-210mm F4.5-6.3 OSS" against the database's "E
        // 55-210mm f/4.5-6.3 OSS" once punctuation is stripped.
        let hit: &LensEntry = match candidates.iter().find(|l| norm(&l.model) == want) {
            Some(l) => l,
            None => candidates.iter().find(|l| {
                let m = norm(&l.model);
                !m.is_empty() && (m.contains(&want) || want.contains(&m))
            })?,
        };
        Some(LensMatch {
            camera: cam.model.clone(),
            lens: lens.to_string(),
            crop: cam.crop,
            focal: if focal > 0.0 { focal } else { 50.0 },
            aperture: if aperture > 0.0 { aperture } else { 8.0 },
            distance: if distance > 0.0 { distance } else { 10.0 },
            entry: hit.clone(),
        })
    }
}

/// "E 55-210mm F4.5-6.3 OSS" and "E 55-210mm f/4.5-6.3 OSS" are the same lens.
fn norm(s: &str) -> String {
    s.to_lowercase()
        .replace("f/", "f")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

// -------------------------------------------------------------------------
// coefficient interpolation
// -------------------------------------------------------------------------

/// Linear interpolation between the two calibrated focal lengths that bracket
/// the one actually used.
fn bracket<T, F: Fn(&T) -> f32>(items: &[T], focal: f32, key: F) -> Option<(usize, usize, f32)> {
    if items.is_empty() {
        return None;
    }
    let mut idx: Vec<usize> = (0..items.len()).collect();
    idx.sort_by(|a, b| key(&items[*a]).partial_cmp(&key(&items[*b])).unwrap());
    let first = idx[0];
    let last = idx[idx.len() - 1];
    if focal <= key(&items[first]) {
        return Some((first, first, 0.0));
    }
    if focal >= key(&items[last]) {
        return Some((last, last, 0.0));
    }
    for w in idx.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let (fl, fh) = (key(&items[lo]), key(&items[hi]));
        if focal >= fl && focal <= fh {
            let t = if (fh - fl).abs() < 1e-6 { 0.0 } else { (focal - fl) / (fh - fl) };
            return Some((lo, hi, t));
        }
    }
    Some((last, last, 0.0))
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DistCoeffs {
    pub model: u8, // 0 none, 1 ptlens, 2 poly3, 3 poly5
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub k1: f32,
    pub k2: f32,
}

pub fn distortion_at(entry: &LensEntry, focal: f32) -> DistCoeffs {
    let (lo, hi, t) = match bracket(&entry.dist, focal, |d| d.f) {
        Some(v) => v,
        None => return DistCoeffs::default(),
    };
    let (a, b) = (&entry.dist[lo], &entry.dist[hi]);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let model = match a.m.as_str() {
        "ptlens" => 1,
        "poly3" => 2,
        "poly5" => 3,
        _ => 0,
    };
    DistCoeffs {
        model,
        a: mix(a.a, b.a),
        b: mix(a.b, b.b),
        c: mix(a.c, b.c),
        k1: mix(a.k1, b.k1),
        k2: mix(a.k2, b.k2),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TcaCoeffs {
    pub model: u8, // 0 none, 1 linear, 2 poly3
    pub br: f32,
    pub cr: f32,
    pub vr: f32,
    pub bb: f32,
    pub cb: f32,
    pub vb: f32,
}

impl Default for TcaCoeffs {
    fn default() -> Self {
        TcaCoeffs { model: 0, br: 0.0, cr: 0.0, vr: 1.0, bb: 0.0, cb: 0.0, vb: 1.0 }
    }
}

pub fn tca_at(entry: &LensEntry, focal: f32) -> TcaCoeffs {
    let (lo, hi, t) = match bracket(&entry.tca, focal, |x| x.f) {
        Some(v) => v,
        None => return TcaCoeffs::default(),
    };
    let (a, b) = (&entry.tca[lo], &entry.tca[hi]);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    match a.m.as_str() {
        "poly3" => TcaCoeffs {
            model: 2,
            br: mix(a.br, b.br),
            cr: mix(a.cr, b.cr),
            vr: mix(a.vr, b.vr),
            bb: mix(a.bb, b.bb),
            cb: mix(a.cb, b.cb),
            vb: mix(a.vb, b.vb),
        },
        "linear" => TcaCoeffs {
            model: 1,
            vr: mix(a.kr, b.kr),
            vb: mix(a.kb, b.kb),
            ..TcaCoeffs::default()
        },
        _ => TcaCoeffs::default(),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VignCoeffs {
    pub found: bool,
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

/// Vignetting is calibrated over focal, aperture *and* focus distance, and the
/// three axes are not independent.
pub fn vignetting_at(entry: &LensEntry, focal: f32, aperture: f32, distance: f32) -> VignCoeffs {
    if entry.vign.is_empty() {
        return VignCoeffs::default();
    }
    let mut focals: Vec<f32> = entry.vign.iter().map(|v| v.f).collect();
    focals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    focals.dedup();
    let (flo, fhi, ft) = bracket_values(&focals, focal);

    let lo = at_focal(entry, flo, aperture, distance);
    if (fhi - flo).abs() < 1e-6 {
        return lo;
    }
    let hi = at_focal(entry, fhi, aperture, distance);
    if !lo.found {
        return hi;
    }
    if !hi.found {
        return lo;
    }
    VignCoeffs {
        found: true,
        k1: lo.k1 + (hi.k1 - lo.k1) * ft,
        k2: lo.k2 + (hi.k2 - lo.k2) * ft,
        k3: lo.k3 + (hi.k3 - lo.k3) * ft,
    }
}

/// The coefficients for one calibrated focal length, interpolated over aperture
/// in stops (which is how apertures are actually spaced) at the nearest
/// calibrated focus distance.
fn at_focal(entry: &LensEntry, focal: f32, aperture: f32, distance: f32) -> VignCoeffs {
    let here: Vec<&VignEntry> = entry.vign.iter().filter(|v| (v.f - focal).abs() < 1e-6).collect();
    if here.is_empty() {
        return VignCoeffs::default();
    }
    let mut dists: Vec<f32> = here.iter().map(|v| v.d).collect();
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    dists.dedup();
    let dist = *dists
        .iter()
        .min_by(|a, b| {
            let key = |x: &f32| (x.max(1e-3).log2() - distance.max(1e-3).log2()).abs();
            key(a).partial_cmp(&key(b)).unwrap()
        })
        .unwrap_or(&distance);
    let subset: Vec<&VignEntry> = here.into_iter().filter(|v| (v.d - dist).abs() < 1e-6).collect();
    if subset.is_empty() {
        return VignCoeffs::default();
    }
    let mut aps: Vec<f32> = subset.iter().map(|v| v.ap).collect();
    aps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    aps.dedup();
    // Interpolate in stops: f/4 to f/5.6 is one step, f/16 to f/22 is one step.
    let stops: Vec<f32> = aps.iter().map(|a| a.max(1e-3).log2()).collect();
    let (slo, shi, t) = bracket_values(&stops, aperture.max(1e-3).log2());
    let pick = |stop: f32| -> Option<&VignEntry> {
        subset.iter().copied().find(|v| (v.ap.max(1e-3).log2() - stop).abs() < 1e-6)
    };
    let (a, b) = match (pick(slo), pick(shi)) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) | (None, Some(a)) => (a, a),
        _ => return VignCoeffs::default(),
    };
    VignCoeffs {
        found: true,
        k1: a.k1 + (b.k1 - a.k1) * t,
        k2: a.k2 + (b.k2 - a.k2) * t,
        k3: a.k3 + (b.k3 - a.k3) * t,
    }
}

/// Bracket a sorted list around `x`, clamping at both ends.  Returns the two
/// values and the fraction between them.
fn bracket_values(sorted: &[f32], x: f32) -> (f32, f32, f32) {
    if sorted.is_empty() {
        return (x, x, 0.0);
    }
    if x <= sorted[0] {
        return (sorted[0], sorted[0], 0.0);
    }
    let last = sorted[sorted.len() - 1];
    if x >= last {
        return (last, last, 0.0);
    }
    for w in sorted.windows(2) {
        if x >= w[0] && x <= w[1] {
            let t = if (w[1] - w[0]).abs() < 1e-9 { 0.0 } else { (x - w[0]) / (w[1] - w[0]) };
            return (w[0], w[1], t);
        }
    }
    (last, last, 0.0)
}

// -------------------------------------------------------------------------
// applying the models
// -------------------------------------------------------------------------

/// Undo the lens' brightness falloff.  Vignetting coordinates are normalised
/// by the half diagonal (verified against liblensfun).
pub fn apply_vignetting(img: &mut Image, m: &LensMatch) -> Value {
    let k = vignetting_at(&m.entry, m.focal, m.aperture, m.distance);
    if !k.found {
        return json!({"applied": false, "reason": "no_vignetting_calibration"});
    }
    let before = corner_mean(img);
    let cx = (img.w as f32 - 1.0) / 2.0;
    let cy = (img.h as f32 - 1.0) / 2.0;
    let half_diag = (cx * cx + cy * cy).sqrt().max(1.0);
    for y in 0..img.h {
        let dy = (y as f32 - cy) / half_diag;
        for x in 0..img.w {
            let dx = (x as f32 - cx) / half_diag;
            let r2 = dx * dx + dy * dy;
            let r4 = r2 * r2;
            let r6 = r4 * r2;
            let c = 1.0 + k.k1 * r2 + k.k2 * r4 + k.k3 * r6;
            let gain = if c > 1e-3 { 1.0 / c } else { 1.0 };
            let i = (y * img.w + x) * 3;
            for ch in 0..3 {
                img.d[i + ch] *= gain;
            }
        }
    }
    let gain = corner_mean(img) / before.max(1e-9);
    json!({"applied": true, "corner_gain_ev": ((gain.max(1e-6).log2() * 1000.0).round() / 1000.0)})
}

fn corner_mean(img: &Image) -> f32 {
    let ch = (img.h / 12).max(1);
    let cw = (img.w / 12).max(1);
    let mut sum = 0f64;
    let mut n = 0usize;
    for (y0, x0) in [(0, 0), (0, img.w - cw), (img.h - ch, 0), (img.h - ch, img.w - cw)] {
        for y in y0..y0 + ch {
            for x in x0..x0 + cw {
                let i = (y * img.w + x) * 3;
                sum += (img.d[i] + img.d[i + 1] + img.d[i + 2]) as f64;
                n += 3;
            }
        }
    }
    (sum / n.max(1) as f64) as f32
}

#[inline]
fn dist_ratio(k: &DistCoeffs, ru: f32) -> f32 {
    match k.model {
        1 => k.a * ru * ru * ru + k.b * ru * ru + k.c * ru + 1.0 - k.a - k.b - k.c,
        2 => 1.0 - k.k1 + k.k1 * ru * ru,
        3 => 1.0 + k.k1 * ru * ru + k.k2 * ru * ru * ru * ru,
        _ => 1.0,
    }
}

/// Resample for distortion and lateral CA. For every destination pixel we
/// compute the source coordinate of each colour channel separately.
pub fn apply_geometry(img: &Image, m: &LensMatch, distortion: bool, tca: bool) -> (Image, Value) {
    let dk = if distortion { distortion_at(&m.entry, m.focal) } else { DistCoeffs::default() };
    let tk = if tca { tca_at(&m.entry, m.focal) } else { TcaCoeffs::default() };
    if dk.model == 0 && tk.model == 0 {
        return (img.clone(), json!({"applied": false, "reason": "no_geometry_calibration"}));
    }

    let (w, h) = (img.w, img.h);
    let cx = (w as f32 - 1.0) / 2.0;
    let cy = (h as f32 - 1.0) / 2.0;
    // Geometry normalisation: half the *shorter* side (verified against liblensfun).
    let unit = (w.min(h) as f32) / 2.0;
    let scale = autoscale(&dk, w, h, unit, cx, cy);

    // Sampled straight from the interleaved frame, rather than from three
    // copies of it split into planes, and once per pixel rather than once per
    // channel when lateral CA is not being corrected, since then all three
    // channels come from the same place.
    let mut out = Image::new(w, h);
    let at = |sx: f32, sy: f32| -> (usize, usize, usize, usize, f32, f32) {
        let x = sx.clamp(0.0, w as f32 - 1.001);
        let y = sy.clamp(0.0, h as f32 - 1.001);
        let (x0, y0) = (x as usize, y as usize);
        (x0, y0, (x0 + 1).min(w - 1), (y0 + 1).min(h - 1), x - x0 as f32, y - y0 as f32)
    };
    let sample = |c: usize, (x0, y0, x1, y1, fx, fy): (usize, usize, usize, usize, f32, f32)| {
        let p = |x: usize, y: usize| img.d[(y * w + x) * 3 + c];
        let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
        let bot = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
        top * (1.0 - fy) + bot * fy
    };
    for y in 0..h {
        let ny = (y as f32 - cy) / unit;
        for x in 0..w {
            let nx = (x as f32 - cx) / unit;
            let ru = (nx * nx + ny * ny).sqrt() / scale;
            let base = if ru > 1e-9 { dist_ratio(&dk, ru) } else { 1.0 };
            let i = (y * w + x) * 3;
            let place = |chan: f32| {
                let f = base * chan / scale;
                at(cx + nx * unit * f, cy + ny * unit * f)
            };
            if tk.model == 0 {
                let p = place(1.0);
                for c in 0..3 {
                    out.d[i + c] = sample(c, p);
                }
                continue;
            }
            for c in 0..3 {
                let chan = match (c, tk.model) {
                    (0, 1) => tk.vr,
                    (2, 1) => tk.vb,
                    (0, 2) => {
                        let r = ru * base;
                        tk.br * r * r + tk.cr * r + tk.vr
                    }
                    (2, 2) => {
                        let r = ru * base;
                        tk.bb * r * r + tk.cb * r + tk.vb
                    }
                    _ => 1.0,
                };
                out.d[i + c] = sample(c, place(chan));
            }
        }
    }
    (out, json!({"applied": true, "distortion": dk.model != 0, "tca": tk.model != 0,
                 "scale": ((scale * 10000.0).round() / 10000.0)}))
}

/// The smallest zoom that leaves no empty edge after correction, lensfun's
/// `MODIFY_SCALE` with an automatic factor.  Monotone in `scale`, so bisect.
fn autoscale(dk: &DistCoeffs, w: usize, h: usize, unit: f32, cx: f32, cy: f32) -> f32 {
    if dk.model == 0 {
        return 1.0;
    }
    let mut samples = Vec::new();
    let steps = 32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        samples.push((t * (w as f32 - 1.0), 0.0));
        samples.push((t * (w as f32 - 1.0), h as f32 - 1.0));
        samples.push((0.0, t * (h as f32 - 1.0)));
        samples.push((w as f32 - 1.0, t * (h as f32 - 1.0)));
    }
    let inside = |scale: f32| -> bool {
        samples.iter().all(|(x, y)| {
            let nx = (x - cx) / unit;
            let ny = (y - cy) / unit;
            let ru = (nx * nx + ny * ny).sqrt() / scale;
            let f = if ru > 1e-9 { dist_ratio(dk, ru) } else { 1.0 } / scale;
            let sx = cx + nx * unit * f;
            let sy = cy + ny * unit * f;
            sx >= 0.0 && sx <= w as f32 - 1.0 && sy >= 0.0 && sy <= h as f32 - 1.0
        })
    };
    if inside(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (1.0f32, 2.0f32);
    if !inside(hi) {
        return hi;
    }
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if inside(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &str) -> Database {
        let full = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/src/lib/wasm/");
        Database::parse(&std::fs::read(format!("{full}{path}")).expect(path)).expect(path)
    }

    /// The web app never holds the whole database.
    #[test]
    fn a_mount_at_a_time_finds_what_the_whole_database_finds() {
        let whole = read("lensfun.json");
        let cameras_only = read("lenses/cameras.json");
        let index: serde_json::Value = serde_json::from_slice(&std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../web/src/lib/wasm/lenses/cameras.json")).unwrap()).unwrap();
        let mut compared = 0;
        for cam in &whole.cameras {
            // A body listed twice is only ever found as its first entry.
            if !std::ptr::eq(whole.find_camera(&cam.maker, &cam.model).unwrap(), cam) {
                continue;
            }
            let mount = cam.mount.to_lowercase();
            let Some(file) = index["chunks"][&mount].as_str() else { continue };
            let mut partial = cameras_only.clone();
            assert_eq!(partial.missing_mount(&cam.maker, &cam.model, "Some Lens"), Some(mount.clone()));
            assert!(partial.find_lens(&cam.maker, &cam.model, "Some Lens", 50.0, 8.0, 10.0).is_none(),
                    "a lens was found before its mount was loaded");
            partial.extend(read(&format!("lenses/{file}.json")));
            assert_eq!(partial.missing_mount(&cam.maker, &cam.model, "Some Lens"), None);
            for lens in whole.lenses.iter().filter(|l| l.mount.to_lowercase() == mount) {
                let a = whole.find_lens(&cam.maker, &cam.model, &lens.model, 50.0, 8.0, 10.0);
                let b = partial.find_lens(&cam.maker, &cam.model, &lens.model, 50.0, 8.0, 10.0);
                assert_eq!(a.map(|m| m.entry.model), b.map(|m| m.entry.model),
                           "{} on a {}", lens.model, cam.model);
                compared += 1;
            }
        }
        assert!(compared > 10_000, "compared only {compared}");
    }

    /// Another mount's lenses already loaded must not stand in for the
    /// right ones while those are still on their way.
    #[test]
    fn another_mount_does_not_answer_for_a_missing_one() {
        let index: serde_json::Value = serde_json::from_slice(&std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../web/src/lib/wasm/lenses/cameras.json")).unwrap()).unwrap();
        let mut partial = read("lenses/cameras.json");
        partial.extend(read(&format!("lenses/{}.json", index["chunks"]["canon ef"].as_str().unwrap())));
        let whole = read("lensfun.json");
        let sony = whole.cameras.iter().find(|c| c.mount.to_lowercase() == "sony e").unwrap();
        assert!(partial.find_lens(&sony.maker, &sony.model, "EF 50mm f/1.8 II", 50.0, 8.0, 10.0).is_none());
        assert_eq!(partial.missing_mount(&sony.maker, &sony.model, "E 16-55mm F2.8 G").as_deref(),
                   Some("sony e"));
    }
}

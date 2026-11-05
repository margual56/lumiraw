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
    /// The crop factor and shape of the sensor the lens was calibrated on.
    #[serde(default = "one")]
    pub crop: f32,
    #[serde(default = "three_two")]
    pub aspect: f32,
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

fn three_two() -> f32 {
    1.5
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
    /// The camera's own corrections, when lensfun does not know the lens;
    /// `entry` is then empty.
    pub embedded: Option<Embedded>,
    /// Found from a focal range and an aperture alone, so maybe the wrong lens.
    pub guessed: bool,
}

impl LensMatch {
    fn lensfun(cam: &CameraEntry, entry: &LensEntry, shown: &str, focal: f32, aperture: f32,
               distance: f32, guessed: bool) -> LensMatch {
        LensMatch {
            camera: cam.model.clone(),
            lens: shown.to_string(),
            crop: cam.crop,
            focal: if focal > 0.0 { focal } else { 50.0 },
            aperture: if aperture > 0.0 { aperture } else { 8.0 },
            distance: if distance > 0.0 { distance } else { 10.0 },
            entry: entry.clone(),
            embedded: None,
            guessed,
        }
    }

    /// A match on the corrections the camera wrote into the file.
    pub fn from_embedded(embedded: Embedded, camera: &str, lens: &str, crop: f32, focal: f32,
                         aperture: f32) -> LensMatch {
        LensMatch {
            camera: camera.to_string(),
            lens: lens.to_string(),
            crop,
            focal,
            aperture,
            distance: 10.0,
            entry: LensEntry {
                maker: String::new(), model: String::new(), mount: String::new(), crop,
                aspect: 1.5, dist: Vec::new(), tca: Vec::new(), vign: Vec::new(),
            },
            embedded: Some(embedded),
            guessed: false,
        }
    }

    /// One unit of lensfun's radius in this image, in pixels: half the
    /// calibration sensor's short side (`diagonal` false, for distortion and
    /// CA) or half its diagonal (vignetting), scaled to this camera's sensor.
    /// A full-frame calibration on an APS-C body reaches past the corners.
    fn calibration_unit(&self, w: usize, h: usize, diagonal: bool) -> f32 {
        let half_diag = ((w as f32 - 1.0).powi(2) + (h as f32 - 1.0).powi(2)).sqrt() / 2.0;
        let sensors = if self.entry.crop > 0.0 && self.crop > 0.0 { self.crop / self.entry.crop } else { 1.0 };
        let shape = if diagonal { 1.0 } else { self.entry.aspect.max(1.0).hypot(1.0) };
        (half_diag * sensors / shape).max(1.0)
    }

    /// Where the calibration came from: `"lensfun"` or `"camera"`.
    pub fn source(&self) -> &'static str {
        if self.embedded.is_some() { "camera" } else { "lensfun" }
    }
}

/// The lens corrections a Sony body writes into every raw (tags 0x7032,
/// 0x7035 and 0x7037), read as darktable reads them: one value per knot, the
/// knots spread evenly over the half diagonal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Embedded {
    /// How far out to sample, as a fraction of the radius, for each knot.
    pub dist: Vec<f32>,
    /// The red and blue channels' radii relative to green, for each knot.
    pub ca_r: Vec<f32>,
    pub ca_b: Vec<f32>,
    /// How bright each knot's radius is relative to the centre.
    pub vign: Vec<f32>,
}

impl Embedded {
    /// From the raw tag values, each starting with its count (twice the knots
    /// for chromatic aberration, red then blue). `None` when they do not agree.
    pub fn from_sony(dist: &[i32], ca: &[i32], vign: &[i32]) -> Option<Embedded> {
        let n = *dist.first()? as usize;
        if !(2..=16).contains(&n) || ca.first() != Some(&(2 * n as i32))
            || vign.first() != Some(&(n as i32))
            || dist.len() <= n || ca.len() <= 2 * n || vign.len() <= n {
            return None;
        }
        let at = |v: &[i32], i: usize, shift: i32| v[i] as f32 * 2f32.powi(-shift) + 1.0;
        Some(Embedded {
            dist: (1..=n).map(|i| at(dist, i, 14)).collect(),
            ca_r: (1..=n).map(|i| at(ca, i, 21)).collect(),
            ca_b: (n + 1..=2 * n).map(|i| at(ca, i, 21)).collect(),
            vign: (1..=n).map(|i| 2f32.powf(0.5 - 2f32.powf(vign[i] as f32 * 2f32.powi(-13) - 1.0)))
                .collect(),
        })
    }

    fn has_distortion(&self) -> bool {
        self.dist.iter().any(|v| (v - 1.0).abs() > 1e-6)
    }
    fn has_tca(&self) -> bool {
        self.ca_r.iter().chain(&self.ca_b).any(|v| (v - 1.0).abs() > 1e-7)
    }
    fn has_vignetting(&self) -> bool {
        self.vign.iter().any(|v| (v - 1.0).abs() > 1e-6)
    }
}

/// A table of values at knots `(i + 0.5) / (n - 1)`, linear between them and
/// flat past either end, at radius `r`.
pub fn spline(values: &[f32], r: f32) -> f32 {
    let n = values.len();
    if n == 0 {
        return 1.0;
    }
    if n == 1 {
        return values[0];
    }
    let pos = r * (n - 1) as f32 - 0.5;
    if pos <= 0.0 {
        return values[0];
    }
    let i = pos as usize;
    if i + 1 >= n {
        return values[n - 1];
    }
    let t = pos - i as f32;
    values[i] + (values[i + 1] - values[i]) * t
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

    /// The mount whose lenses this camera's lens has to be looked up among,
    /// when they have not been loaded. Wanted even with no lens named, for
    /// choosing one by hand.
    pub fn missing_mount(&self, make: &str, model: &str) -> Option<String> {
        let loaded = self.mounts.as_ref()?;
        let mount = self.find_camera(make, model)?.mount.to_lowercase();
        (!loaded.contains(&mount)).then_some(mount)
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

    /// The camera, and the calibrated lenses that fit it. `None` while its
    /// mount's part of the database has not arrived: another mount's lenses
    /// must not answer in the meantime.
    fn candidates(&self, make: &str, model: &str) -> Option<(&CameraEntry, Vec<&LensEntry>)> {
        let cam = self.find_camera(make, model)?;
        let mount = cam.mount.to_lowercase();
        if self.mounts.as_ref().is_some_and(|loaded| !loaded.contains(&mount)) {
            return None;
        }
        let mut lenses: Vec<&LensEntry> = self.lenses.iter()
            .filter(|l| mount.is_empty() || l.mount.to_lowercase() == mount)
            .collect();
        if lenses.is_empty() {
            lenses = self.lenses.iter().collect();
        }
        Some((cam, lenses))
    }

    /// Every calibrated lens for this camera's mount, each once, by maker and
    /// model: what a lens can be chosen from.
    pub fn lenses_for(&self, make: &str, model: &str) -> Vec<LensEntry> {
        let Some((_, mut lenses)) = self.candidates(make, model) else { return Vec::new() };
        lenses.sort_by_key(|l| (l.maker.to_lowercase(), l.model.to_lowercase()));
        lenses.dedup_by_key(|l| (norm(&l.maker), norm(&l.model)));
        lenses.into_iter().cloned().collect()
    }

    pub fn find_lens(&self, make: &str, model: &str, lens: &str, focal: f32, aperture: f32,
                     distance: f32) -> Option<LensMatch> {
        let (cam, candidates) = self.candidates(make, model)?;
        let want = norm(lens);
        if want.is_empty() {
            return None;
        }
        // Exact name first; then either name containing the other, which is
        // what catches "E 55-210mm F4.5-6.3 OSS" against the database's "E
        // 55-210mm f/4.5-6.3 OSS" once punctuation is stripped.
        if let Some(hit) = candidates.iter().find(|l| norm(&l.model) == want) {
            return Some(LensMatch::lensfun(cam, hit, lens, focal, aperture, distance, false));
        }
        // A name that is nothing but a focal range and an aperture is how Sony
        // bodies report other makers' lenses ("E 18-50mm F2.8" for Sigma's
        // 18-50mm F2.8 DC DN). Contained in "E 35mm f/1.8 OSS" it is still
        // not that lens; it can only be the mount's one lens with that range
        // and aperture, if there is only one, and that is a guess.
        let bare = spec(lens).filter(|(_, rest)| rest.iter().all(|t| MOUNT_PREFIXES.contains(&t.as_str())));
        let Some((want_spec, _)) = bare else {
            let hit = candidates.iter().find(|l| {
                let m = norm(&l.model);
                !m.is_empty() && (m.contains(&want) || want.contains(&m))
            })?;
            return Some(LensMatch::lensfun(cam, hit, lens, focal, aperture, distance, false));
        };
        let same: Vec<&LensEntry> = candidates.into_iter()
            .filter(|l| spec(&l.model).is_some_and(|(s, _)| same_spec(s, want_spec)))
            .collect();
        let hit = only_one(&same)?;
        Some(LensMatch::lensfun(cam, hit, &full_name(hit), focal, aperture, distance, true))
    }

    /// The lens the maker's own lens number names ("Sigma", "18-50mm F2.8 DC
    /// DN | C"), in lensfun, whose names for the same lens differ ("18-50mm
    /// F2.8 DC DN | Contemporary 021"): the same maker and specification, and
    /// of those the one sharing most words with it.
    pub fn find_identified(&self, make: &str, model: &str, maker: &str, lens: &str, focal: f32,
                           aperture: f32, distance: f32) -> Option<LensMatch> {
        let (cam, candidates) = self.candidates(make, model)?;
        let (want_spec, _) = spec(lens)?;
        let maker_n = norm(maker);
        let wanted = words(lens);
        let mut scored: Vec<(&LensEntry, usize)> = candidates.into_iter()
            .filter(|l| norm(&l.maker) == maker_n)
            .filter(|l| spec(&l.model).is_some_and(|(s, _)| same_spec(s, want_spec)))
            .map(|l| (l, words(&l.model).iter().filter(|w| wanted.contains(w)).count()))
            .collect();
        let best = scored.iter().map(|(_, n)| *n).max()?;
        scored.retain(|(_, n)| *n == best);
        let tied: Vec<&LensEntry> = scored.into_iter().map(|(l, _)| l).collect();
        let hit = only_one(&tied)?;
        Some(LensMatch::lensfun(cam, hit, &full_name(hit), focal, aperture, distance, false))
    }

    /// A lens chosen by hand, by its model name in the database.
    pub fn find_chosen(&self, make: &str, model: &str, chosen: &str, focal: f32, aperture: f32,
                       distance: f32) -> Option<LensMatch> {
        let (cam, candidates) = self.candidates(make, model)?;
        let hit = candidates.into_iter().find(|l| l.model == chosen)?;
        Some(LensMatch::lensfun(cam, hit, &full_name(hit), focal, aperture, distance, false))
    }
}

/// Whether two lens names give different focal ranges or apertures; false
/// when either gives none.
pub fn specs_differ(a: &str, b: &str) -> bool {
    matches!((spec(a), spec(b)), (Some((x, _)), Some((y, _))) if !same_spec(x, y))
}

/// The lens, if every entry is the same one: listed twice under the same name
/// is still the one lens.
fn only_one<'a>(lenses: &[&'a LensEntry]) -> Option<&'a LensEntry> {
    let first = *lenses.first()?;
    lenses.iter().all(|l| norm(&l.model) == norm(&first.model)).then_some(first)
}

/// "Sigma 18-50mm F2.8 DC DN | Contemporary 021", with the maker in front
/// unless the model already starts with it.
pub fn full_name(l: &LensEntry) -> String {
    if l.model.to_lowercase().starts_with(&l.maker.to_lowercase()) {
        l.model.clone()
    } else {
        format!("{} {}", l.maker, l.model)
    }
}

/// The words of a lens name, with Sigma's line letters spelt out.
fn words(name: &str) -> Vec<String> {
    name.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '.')
        .filter(|w| !w.is_empty())
        .map(|w| match w {
            "c" => "contemporary".to_string(),
            "a" => "art".to_string(),
            "s" => "sports".to_string(),
            _ => w.to_string(),
        })
        .collect()
}

/// What a lens name may carry besides its specification and still say
/// nothing about which lens it is.
const MOUNT_PREFIXES: &[&str] = &["e", "fe", "dt", "pz"];

/// (shortest focal, longest focal, widest aperture)
type Spec = (f32, f32, f32);

fn same_spec(a: Spec, b: Spec) -> bool {
    (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05 && (a.2 - b.2).abs() < 0.05
}

/// A lens name's focal range and widest aperture, "18-50mm F2.8" or "50mm
/// f/1.8", with the words around them.
fn spec(name: &str) -> Option<(Spec, Vec<String>)> {
    let lower = name.to_lowercase().replace("f/", "f");
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let range = |s: &str| -> Option<(f32, f32)> {
        match s.split_once('-') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => {
                let v: f32 = s.parse().ok()?;
                Some((v, v))
            }
        }
    };
    for i in 0..tokens.len().saturating_sub(1) {
        let Some(focal) = tokens[i].strip_suffix("mm").and_then(range) else { continue };
        let Some(ap) = tokens[i + 1].strip_prefix('f').and_then(range) else { continue };
        let rest = tokens.iter().enumerate()
            .filter(|(j, _)| *j != i && *j != i + 1)
            .map(|(_, t)| t.to_string())
            .collect();
        return Some(((focal.0, focal.1, ap.0), rest));
    }
    None
}

/// "E 55-210mm F4.5-6.3 OSS" and "E 55-210mm f/4.5-6.3 OSS" are the same lens.
fn norm(s: &str) -> String {
    s.to_lowercase()
        .replace("f/", "f")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

// Coefficient interpolation
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

// Applying the models
/// Undo the lens' brightness falloff. Radii are over the half diagonal of the
/// sensor the lens was calibrated on, as liblensfun has them.
pub fn apply_vignetting(img: &mut Image, m: &LensMatch) -> Value {
    let k = vignetting_at(&m.entry, m.focal, m.aperture, m.distance);
    let embedded = m.embedded.as_ref().filter(|e| e.has_vignetting());
    if !k.found && embedded.is_none() {
        return json!({"applied": false, "reason": "no_vignetting_calibration"});
    }
    let before = corner_mean(img);
    let cx = (img.w as f32 - 1.0) / 2.0;
    let cy = (img.h as f32 - 1.0) / 2.0;
    // The camera's tables are for its own sensor; lensfun's for the one it
    // was calibrated on.
    let unit = match embedded {
        Some(_) => (cx * cx + cy * cy).sqrt().max(1.0),
        None => m.calibration_unit(img.w, img.h, true),
    };
    for y in 0..img.h {
        let dy = (y as f32 - cy) / unit;
        for x in 0..img.w {
            let dx = (x as f32 - cx) / unit;
            let r2 = dx * dx + dy * dy;
            let c = match embedded {
                Some(e) => spline(&e.vign, r2.sqrt()),
                None => {
                    let r4 = r2 * r2;
                    1.0 + k.k1 * r2 + k.k2 * r4 + k.k3 * r4 * r2
                }
            };
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
    if let Some(e) = &m.embedded {
        return apply_embedded_geometry(img, e, distortion, tca);
    }
    let dk = if distortion { distortion_at(&m.entry, m.focal) } else { DistCoeffs::default() };
    let tk = if tca { tca_at(&m.entry, m.focal) } else { TcaCoeffs::default() };
    if dk.model == 0 && tk.model == 0 {
        return (img.clone(), json!({"applied": false, "reason": "no_geometry_calibration"}));
    }

    let (w, h) = (img.w, img.h);
    let cx = (w as f32 - 1.0) / 2.0;
    let cy = (h as f32 - 1.0) / 2.0;
    let unit = m.calibration_unit(w, h, false);
    let scale = if dk.model == 0 { 1.0 } else { autoscale(&|ru| dist_ratio(&dk, ru), w, h, unit, cx, cy) };

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

/// Distortion and lateral CA from the camera's own tables. Radii are over the
/// half diagonal; each output pixel samples the frame `dist(r)` times as far
/// out, red and blue further scaled by their own tables.
fn apply_embedded_geometry(img: &Image, e: &Embedded, distortion: bool, tca: bool) -> (Image, Value) {
    let distortion = distortion && e.has_distortion();
    let tca = tca && e.has_tca();
    if !distortion && !tca {
        return (img.clone(), json!({"applied": false, "reason": "no_geometry_calibration"}));
    }
    let (w, h) = (img.w, img.h);
    let cx = (w as f32 - 1.0) / 2.0;
    let cy = (h as f32 - 1.0) / 2.0;
    let unit = (cx * cx + cy * cy).sqrt().max(1.0);
    let dist = |r: f32| if distortion { spline(&e.dist, r) } else { 1.0 };
    let scale = if distortion { autoscale(&dist, w, h, unit, cx, cy) } else { 1.0 };

    let mut out = Image::new(w, h);
    for y in 0..h {
        let ny = (y as f32 - cy) / unit;
        for x in 0..w {
            let nx = (x as f32 - cx) / unit;
            let ru = (nx * nx + ny * ny).sqrt() / scale;
            let base = dist(ru) / scale;
            let i = (y * w + x) * 3;
            for c in 0..3 {
                let chan = match (tca, c) {
                    (true, 0) => spline(&e.ca_r, ru),
                    (true, 2) => spline(&e.ca_b, ru),
                    _ => 1.0,
                };
                let f = base * chan;
                out.d[i + c] = bilinear(img, c, cx + nx * unit * f, cy + ny * unit * f);
            }
        }
    }
    (out, json!({"applied": true, "distortion": distortion, "tca": tca,
                 "scale": ((scale * 10000.0).round() / 10000.0)}))
}

/// Channel `c` of `img` at a fractional position, clamped to the frame.
fn bilinear(img: &Image, c: usize, sx: f32, sy: f32) -> f32 {
    let (w, h) = (img.w, img.h);
    let x = sx.clamp(0.0, w as f32 - 1.001);
    let y = sy.clamp(0.0, h as f32 - 1.001);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |x: usize, y: usize| img.d[(y * w + x) * 3 + c];
    let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
    let bot = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
    top * (1.0 - fy) + bot * fy
}

/// The smallest zoom that leaves no empty edge after correction, lensfun's
/// `MODIFY_SCALE` with an automatic factor.  Monotone in `scale`, so bisect.
/// `ratio` is how far out a radius (in `unit`s) samples, relative to itself.
fn autoscale(ratio: &dyn Fn(f32) -> f32, w: usize, h: usize, unit: f32, cx: f32, cy: f32) -> f32 {
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
            let f = if ru > 1e-9 { ratio(ru) } else { 1.0 } / scale;
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
        let mut seen = std::collections::HashSet::new();
        for cam in &whole.cameras {
            // A body listed twice is only ever found as its first entry.
            if !std::ptr::eq(whole.find_camera(&cam.maker, &cam.model).unwrap(), cam) {
                continue;
            }
            let mount = cam.mount.to_lowercase();
            if !seen.insert(mount.clone()) {
                continue;
            }
            let Some(file) = index["chunks"][&mount].as_str() else { continue };
            let mut partial = cameras_only.clone();
            assert_eq!(partial.missing_mount(&cam.maker, &cam.model), Some(mount.clone()));
            assert!(partial.find_lens(&cam.maker, &cam.model, "Some Lens", 50.0, 8.0, 10.0).is_none(),
                    "a lens was found before its mount was loaded");
            partial.extend(read(&format!("lenses/{file}.json")));
            assert_eq!(partial.missing_mount(&cam.maker, &cam.model), None);
            for lens in whole.lenses.iter().filter(|l| l.mount.to_lowercase() == mount) {
                let a = whole.find_lens(&cam.maker, &cam.model, &lens.model, 50.0, 8.0, 10.0);
                let b = partial.find_lens(&cam.maker, &cam.model, &lens.model, 50.0, 8.0, 10.0);
                assert_eq!(a.map(|m| m.entry.model), b.map(|m| m.entry.model),
                           "{} on a {}", lens.model, cam.model);
                compared += 1;
            }
        }
        assert!(compared > 1_000, "compared only {compared}");
    }

    /// Sony bodies name a third-party lens by its focal range and aperture
    /// alone; the one lens of the mount with those is the one meant.
    #[test]
    fn a_lens_named_only_by_its_specification_is_found() {
        let whole = read("lensfun.json");
        let m = whole.find_lens("SONY", "ILCE-5000", "E 18-50mm F2.8", 18.7, 2.8, 10.0).unwrap();
        assert_eq!(m.entry.model, "18-50mm F2.8 DC DN | Contemporary 021");
        assert_eq!(m.lens, "Sigma 18-50mm F2.8 DC DN | Contemporary 021");
        // Sony's own lenses still go by name.
        let own = whole.find_lens("SONY", "ILCE-5000", "E 55-210mm F4.5-6.3 OSS", 55.0, 4.5, 10.0).unwrap();
        assert_eq!(own.lens, "E 55-210mm F4.5-6.3 OSS");
        // A name with more in it than the specification is not guessed at.
        assert!(whole.find_lens("SONY", "ILCE-5000", "E 18-50mm F2.8 XYZ", 18.7, 2.8, 10.0).is_none());
    }

    /// The maker's lens number names the lens in its own words; lensfun's
    /// differ, but the maker, the specification and the words agree.
    #[test]
    fn an_identified_lens_is_found_under_lensfuns_name() {
        let whole = read("lensfun.json");
        let m = whole.find_identified("SONY", "ILCE-5000", "Sigma", "18-50mm F2.8 DC DN | C",
                                      18.7, 2.8, 10.0).unwrap();
        assert_eq!(m.entry.model, "18-50mm F2.8 DC DN | Contemporary 021");
        assert!(!m.guessed);
        let own = whole.find_identified("SONY", "ILCE-5000", "Sony", "E PZ 16-50mm F3.5-5.6 OSS",
                                        16.0, 3.5, 10.0).unwrap();
        assert_eq!(own.entry.model, "E PZ 16-50mm f/3.5-5.6 OSS");
        assert!(whole.find_identified("SONY", "ILCE-5000", "Viltrox", "40mm F2.5 FE Air",
                                      40.0, 2.5, 10.0).is_none());
    }

    /// "E 35mm F1.8" is contained in "E 35mm f/1.8 OSS", but three lenses on
    /// the mount are 35mm f/1.8, and the name says which of them it is not.
    #[test]
    fn a_bare_name_is_not_matched_by_being_contained() {
        let whole = read("lensfun.json");
        assert!(whole.find_lens("SONY", "ILCE-5000", "E 35mm F1.8", 35.0, 1.8, 10.0).is_none());
        let m = whole.find_lens("SONY", "ILCE-5000", "E 18-50mm F2.8", 18.7, 2.8, 10.0).unwrap();
        assert!(m.guessed);
    }

    #[test]
    fn two_lenses_with_the_same_specification_are_not_guessed_between() {
        let lens = |model: &str| LensEntry {
            maker: "Someone".into(), model: model.into(), mount: "Sony E".into(), crop: 1.5,
            aspect: 1.5, dist: Vec::new(), tca: Vec::new(), vign: Vec::new(),
        };
        let db = Database {
            cameras: vec![CameraEntry { maker: "Sony".into(), model: "ILCE-5000".into(),
                                        mount: "Sony E".into(), crop: 1.534 }],
            lenses: vec![lens("30mm F1.4 DC DN"), lens("30mm F1.4 Something Else")],
            mounts: None,
        };
        assert!(db.find_lens("SONY", "ILCE-5000", "E 30mm F1.4", 30.0, 1.4, 10.0).is_none());
    }

    /// A lens calibrated on full frame, on a body with a smaller sensor, sees
    /// only the middle of its field: the APS-C picture corrects as that part
    /// of the full-frame picture does, point for point.
    #[test]
    fn a_full_frame_calibration_fits_a_smaller_sensor() {
        let entry = LensEntry {
            maker: "Someone".into(), model: "50mm F2".into(), mount: "Sony E".into(), crop: 1.0,
            aspect: 1.5,
            dist: vec![DistEntry { f: 50.0, m: "poly5".into(), k1: -0.03, ..DistEntry::default() }],
            tca: Vec::new(),
            vign: vec![VignEntry { f: 50.0, ap: 2.0, d: 10.0, k1: -0.3, k2: 0.0, k3: 0.0 }],
        };
        let on = |crop: f32| LensMatch {
            camera: "body".into(), lens: "50mm F2".into(), crop, focal: 50.0, aperture: 2.0,
            distance: 10.0, entry: entry.clone(), embedded: None, guessed: false,
        };
        // The same pixels: a full-frame picture, and its middle as APS-C has it.
        let (fw, fh, cw, ch) = (901, 601, 601, 401);
        let dot = |w: usize, h: usize| {
            let mut img = Image::new(w, h);
            let (x, y) = (w / 2 + 150, h / 2 + 100);
            img.d[(y * w + x) * 3..][..3].fill(1.0);
            img
        };
        let found = |img: &Image| {
            let i = (0..img.w * img.h).max_by(|a, b| img.d[a * 3].total_cmp(&img.d[b * 3])).unwrap();
            ((i % img.w) as i32 - (img.w / 2) as i32, (i / img.w) as i32 - (img.h / 2) as i32)
        };
        let (full, _) = apply_geometry(&dot(fw, fh), &on(1.0), true, false);
        let (aps, _) = apply_geometry(&dot(cw, ch), &on(1.5), true, false);
        assert_eq!(found(&full), found(&aps));

        let flat = |w, h| { let mut i = Image::new(w, h); i.d.fill(0.5); i };
        let (mut full, mut aps) = (flat(fw, fh), flat(cw, ch));
        apply_vignetting(&mut full, &on(1.0));
        apply_vignetting(&mut aps, &on(1.5));
        let at = |img: &Image, dx: usize, dy: usize| img.d[((img.h / 2 + dy) * img.w + img.w / 2 + dx) * 3];
        assert!((at(&full, 280, 190) - at(&aps, 280, 190)).abs() < 1e-3,
                "{} against {}", at(&full, 280, 190), at(&aps, 280, 190));
    }

    /// The tables from a Sigma 18-50mm F2.8 at 18.7mm, as an ILCE-5000 wrote them.
    fn sigma_tables() -> Embedded {
        Embedded::from_sony(
            &[11, 18, 0, -57, -137, -252, -392, -564, -751, -934, -1078, -1175, 0, 0, 0, 0, 0],
            &[22, 1408, 1280, 1024, 1024, 896, 896, 768, 768, 640, 384, -256, -256, 0, 128, 256,
              384, 256, 256, 384, 512, 896, 1536, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            &[11, 0, 192, 256, 384, 640, 1280, 2368, 4224, 6656, 9472, 12672, 0, 0, 0, 0, 0],
        ).unwrap()
    }

    #[test]
    fn the_cameras_tables_decode() {
        let e = sigma_tables();
        assert_eq!((e.dist.len(), e.ca_r.len(), e.ca_b.len(), e.vign.len()), (11, 11, 11, 11));
        assert!((spline(&e.dist, 0.0) - (1.0 + 18.0 / 16384.0)).abs() < 1e-6);
        assert!((spline(&e.dist, 1.2) - (1.0 - 1175.0 / 16384.0)).abs() < 1e-6);
        assert!((e.ca_b[0] - (1.0 - 256.0 / 2097152.0)).abs() < 1e-7);
        assert_eq!(e.vign[0], 1.0);
        assert!(e.vign[10] < 0.6, "the corner is darker: {}", e.vign[10]);
        assert!(Embedded::from_sony(&[11, 0], &[22], &[11]).is_none());
    }

    /// Barrel distortion pulls the corners in; undoing it pushes a dot near
    /// one further out.
    #[test]
    fn the_cameras_tables_undo_barrel_distortion() {
        let (w, h) = (300, 200);
        let mut img = Image::new(w, h);
        let at = |img: &mut Image, x: usize, y: usize| img.d[(y * w + x) * 3..][..3].fill(1.0);
        at(&mut img, 40, 30);
        let m = LensMatch::from_embedded(sigma_tables(), "ILCE-5000", "E 18-50mm F2.8", 1.534, 18.7, 2.8);
        let (out, info) = apply_geometry(&img, &m, true, false);
        assert_eq!(info["applied"], true);
        let brightest = (0..w * h).max_by(|a, b| out.d[a * 3].total_cmp(&out.d[b * 3])).unwrap();
        let (x, y) = (brightest % w, brightest / w);
        assert!(x < 40 && y < 30, "the dot went to {x},{y}");
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
        assert_eq!(partial.missing_mount(&sony.maker, &sony.model).as_deref(),
                   Some("sony e"));
    }
}

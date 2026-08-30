//! The one place the raw decoder is spoken to.

use rawler::decoders::{Orientation, RawDecodeParams};
use rawler::imgop::xyz::Illuminant;
use rawler::rawimage::{RawImageData, RawPhotometricInterpretation};
use rawler::rawsource::RawSource;
use rawler::RawlerError;

pub use rawler::CFA;
use kit::{Mat3, Matrix3};

/// Why a file could not be read, in a form the interface can say something
/// useful about.
#[derive(Clone, Debug)]
pub struct DecodeError {
    /// `unsupported_camera`, `unsupported_sensor` or `undecodable`.
    pub code: &'static str,
    pub message: String,
    pub make: String,
    pub model: String,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<DecodeError> for String {
    fn from(e: DecodeError) -> String {
        e.message
    }
}

impl DecodeError {
    pub fn undecodable(message: impl Into<String>) -> DecodeError {
        DecodeError { code: "undecodable", message: message.into(),
                      make: String::new(), model: String::new() }
    }
}

/// How the photosites are laid out.
#[derive(Clone, Debug)]
pub enum Layout {
    /// One value per photosite under a colour filter array.
    Mosaic(CFA),
    /// Three values per pixel, already demosaiced: a linear DNG, which is
    /// what iPhone ProRAW and most "merged" DNGs are.
    Linear,
}

/// A decoded raw file, before any of our own processing.
pub struct Raw {
    pub make: String,
    pub model: String,
    /// Size of `data`, in pixels.
    pub width: usize,
    pub height: usize,
    /// Values per pixel in `data`: 1 for a mosaic, 3 for a linear raw.
    pub cpp: usize,
    /// The sensor values as stored, black level included.
    pub data: Vec<f32>,
    pub layout: Layout,
    /// The camera's own white balance, as multipliers with green at one.
    pub wb: [f32; 3],
    /// Whether `wb` came from the file or was made up because it said nothing.
    pub wb_from_file: bool,
    /// XYZ -> camera, per calibration illuminant, as correlated colour
    /// temperature in kelvin.
    pub matrices: Vec<(f32, Mat3)>,
    /// The part of `data` worth keeping: x, y, width, height.
    pub area: (usize, usize, usize, usize),
    pub orientation: Orientation,
    black: BlackPattern,
    white: Vec<f32>,
}

/// Black level, repeating over the frame in a small tile.
struct BlackPattern {
    levels: Vec<f32>,
    w: usize,
    h: usize,
    cpp: usize,
}

impl Raw {
    /// A sensor made up for a test: one black and one white level for every
    /// photosite, no crop, no rotation.
    #[cfg(test)]
    pub(crate) fn synthetic(width: usize, height: usize, data: Vec<f32>, layout: Layout,
                            wb: [f32; 3], matrices: Vec<(f32, Mat3)>,
                            black: f32, white: f32) -> Raw {
        let cpp = match layout { Layout::Mosaic(_) => 1, Layout::Linear => 3 };
        Raw {
            make: String::new(), model: String::new(), width, height, cpp, data, layout, wb,
            wb_from_file: true, matrices, area: (0, 0, width, height),
            orientation: Orientation::Normal,
            black: BlackPattern { levels: vec![black], w: 1, h: 1, cpp: 1 },
            white: vec![white],
        }
    }

    /// The black level at a photosite, for one of its components.
    pub fn black_at(&self, x: usize, y: usize, c: usize) -> f32 {
        let b = &self.black;
        if b.levels.is_empty() {
            return 0.0;
        }
        let i = ((y % b.h.max(1)) * b.w.max(1) + x % b.w.max(1)) * b.cpp.max(1) + c.min(b.cpp.max(1) - 1);
        b.levels.get(i).copied().unwrap_or(b.levels[0])
    }

    /// The level the sensor saturates at, for a colour (0, 1, 2) or, on a
    /// linear raw, a component.
    pub fn white_for(&self, c: usize) -> f32 {
        self.white.get(c).or(self.white.first()).copied().unwrap_or(65535.0)
    }

    /// The value at a photosite scaled so black is 0 and saturation is 1, not
    /// clamped: a value below black is noise and callers decide about it.
    pub fn normalised(&self, x: usize, y: usize, c: usize) -> f32 {
        let colour = match &self.layout {
            Layout::Mosaic(cfa) => cfa.color_at(y, x).min(2),
            Layout::Linear => c,
        };
        let black = self.black_at(x, y, c);
        let v = self.data[(y * self.width + x) * self.cpp + c];
        (v - black) / (self.white_for(colour) - black).max(1.0)
    }
}

/// Where this sensor really saturates, when the file says lower.
const LINEAR_SHARE: f32 = 0.98;

fn observed_ceiling(data: &[f32], width: usize, area: (usize, usize, usize, usize),
                    declared: Option<f32>) -> Option<f32> {
    let declared = declared?;
    let (x0, y0, w, h) = area;
    let mut above: Vec<f32> = Vec::new();
    for y in y0..y0 + h {
        for &v in &data[y * width + x0..y * width + x0 + w] {
            if v > declared {
                above.push(v);
            }
        }
    }
    // A few in a million, and never fewer than sixteen: hot pixels come in
    // ones and twos, a clipped sky in thousands.
    let rank = ((w * h) as f64 * 2e-6).ceil().max(16.0) as usize;
    if above.len() <= rank {
        return None;
    }
    let k = above.len() - rank;
    let (_, value, _) = above.select_nth_unstable_by(k, |a, b| a.total_cmp(b));
    Some(*value)
}

/// CIE 1931 temperature of the illuminants a DNG calibration can name.
fn kelvin(i: &Illuminant) -> Option<f32> {
    Some(match i {
        Illuminant::A | Illuminant::Tungsten | Illuminant::IsoStudioTungsten => 2856.0,
        Illuminant::B => 4874.0,
        Illuminant::C => 6774.0,
        Illuminant::D50 => 5003.0,
        Illuminant::D55 | Illuminant::Daylight | Illuminant::FineWeather | Illuminant::Flash => 5503.0,
        Illuminant::D65 | Illuminant::CloudyWeather => 6504.0,
        Illuminant::D75 | Illuminant::Shade => 7504.0,
        Illuminant::CoolWhiteFluorescent => 4150.0,
        Illuminant::WhiteFluorescent => 3450.0,
        Illuminant::DaylightFluorescent => 6430.0,
        Illuminant::DaylightWhiteFluorescent => 5000.0,
        _ => return None,
    })
}

pub fn load(bytes: &[u8]) -> Result<Raw, DecodeError> {
    let source = RawSource::new_from_slice(bytes);
    let raw = rawler::decode(&source, &RawDecodeParams::default()).map_err(|e| match e {
        // With no camera named, the decoder did not recognise the file as a
        // raw at all, and "your camera is not supported" would be a guess.
        RawlerError::Unsupported { what, make, .. } if make.trim().is_empty() => {
            DecodeError::undecodable(what)
        }
        RawlerError::Unsupported { what, model, make, .. } => DecodeError {
            code: "unsupported_camera",
            message: format!("{what} ({make} {model})"),
            make,
            model,
        },
        RawlerError::DecoderFailed(msg) => DecodeError::undecodable(msg),
    })?;

    let layout = match (&raw.photometric, raw.cpp) {
        (RawPhotometricInterpretation::Cfa(config), 1) => {
            let cfa = config.cfa.clone();
            // A four-colour mosaic (CYGM, RGBE) needs a demosaic and a matrix
            // of its own, and nobody has shot one in fifteen years.
            if !cfa.is_rgb() {
                return Err(DecodeError {
                    code: "unsupported_sensor",
                    message: format!("a {} colour filter is not supported", cfa.name),
                    make: raw.clean_make.clone(),
                    model: raw.clean_model.clone(),
                });
            }
            Layout::Mosaic(cfa)
        }
        (RawPhotometricInterpretation::LinearRaw, 3) => Layout::Linear,
        _ => {
            return Err(DecodeError {
                code: "unsupported_sensor",
                message: format!("{} values per pixel, and not a colour mosaic", raw.cpp),
                make: raw.clean_make.clone(),
                model: raw.clean_model.clone(),
            })
        }
    };

    let data: Vec<f32> = match &raw.data {
        RawImageData::Integer(v) => v.iter().map(|x| *x as f32).collect(),
        RawImageData::Float(v) => v.clone(),
    };

    let mut matrices = Vec::new();
    for (illuminant, flat) in raw.color_matrix.iter() {
        if flat.len() < 9 {
            continue;
        }
        if let Some(k) = kelvin(illuminant) {
            let m = [[flat[0], flat[1], flat[2]], [flat[3], flat[4], flat[5]], [flat[6], flat[7], flat[8]]];
            if m.iter().flatten().any(|v| *v != 0.0) {
                matrices.push((k, Matrix3(m)));
            }
        }
    }
    matrices.sort_by(|a, b| a.0.total_cmp(&b.0));

    // The camera's white balance.
    let from_file = raw.wb_coeffs[..3].iter().all(|v| v.is_finite() && *v > 0.0);
    let coeffs = if from_file { raw.wb_coeffs } else { raw.neutralwb() };
    let mut wb = [coeffs[0], coeffs[1], coeffs[2]];
    for g in wb.iter_mut() {
        if !g.is_finite() || *g <= 0.0 {
            *g = 1.0;
        }
    }
    let green = wb[1];
    for g in wb.iter_mut() {
        *g /= green;
    }

    // The usable area.
    let (fw, fh) = (raw.width, raw.height);
    let mut area = (0, 0, fw, fh);
    if let Some(r) = raw.crop_area.or(raw.active_area) {
        let (x, y, w, h) = (r.p.x, r.p.y, r.d.w, r.d.h);
        if w >= 16 && h >= 16 && x + w <= fw && y + h <= fh {
            area = (x, y, w, h);
        }
    }

    let black = BlackPattern {
        levels: raw.blacklevel.as_vec(),
        w: raw.blacklevel.width,
        h: raw.blacklevel.height,
        cpp: raw.blacklevel.cpp,
    };
    let mut white = raw.whitelevel.as_vec();
    if let Layout::Mosaic(_) = layout {
        let black = raw.blacklevel.as_vec().first().copied().unwrap_or(0.0);
        if let Some(ceiling) = observed_ceiling(&data, raw.width, area, white.first().copied()) {
            let linear_to = black + LINEAR_SHARE * (ceiling - black);
            if linear_to > white.first().copied().unwrap_or(f32::MAX) {
                white = vec![linear_to; white.len().max(1)];
            }
        }
    }

    Ok(Raw {
        make: raw.clean_make.clone(),
        model: raw.clean_model.clone(),
        width: fw,
        height: fh,
        cpp: raw.cpp,
        data,
        layout,
        wb,
        wb_from_file: from_file,
        matrices,
        area,
        orientation: raw.orientation,
        black,
        white,
    })
}

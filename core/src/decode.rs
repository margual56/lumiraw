//! Raw decoding: file bytes -> linear sRGB, plus the EXIF the pipeline reasons about.

use crate::ops::{smoothstep, Image, Plane};
use crate::raw::{self, CFA};
pub use crate::raw::DecodeError;
use rawler::decoders::Orientation;

#[derive(Default, Clone, Debug)]
pub struct Meta {
    pub make: String,
    pub model: String,
    pub lens_model: String,
    pub iso: f32,
    pub aperture: f32,
    pub focal_mm: f32,
    pub focal35_mm: f32,
    pub shutter_s: f32,
    pub exposure_comp: f32,
    pub flash: bool,
    pub orientation: u16,
    /// When the shutter opened, in seconds, or zero when the file does not say.
    pub captured_s: i64,
}

pub struct Decoded {
    pub img: Image,
    pub meta: Meta,
}

/// sRGB(D65) primaries -> XYZ, the same constant dcraw uses.
const XYZ_RGB: [[f32; 3]; 3] = [
    [0.412453, 0.357580, 0.180423],
    [0.212671, 0.715160, 0.072169],
    [0.019334, 0.119193, 0.950227],
];

pub fn decode(bytes: &[u8]) -> Result<Decoded, DecodeError> {
    let raw = raw::load(bytes)?;

    let mut meta = read_exif(bytes);
    if meta.make.is_empty() {
        meta.make = raw.make.clone();
    }
    if meta.model.is_empty() {
        meta.model = raw.model.clone();
    }
    Ok(develop_raw(&raw, meta))
}

/// Sensor values to linear sRGB: levels, white balance, demosaic, highlight
/// reconstruction, colour matrix, orientation.
fn develop_raw(raw: &raw::Raw, meta: Meta) -> Decoded {
    // --- black/white levels + camera white balance ------------------------
    // dcraw normalises the multipliers by the largest one, so no channel can
    // clip further than the raw already did.
    let mut wb = raw.wb;
    let wmax = wb[0].max(wb[1]).max(wb[2]);
    for g in wb.iter_mut() {
        *g /= wmax;
    }

    let (x0, y0, w, h) = raw.area;
    let mut cam = match &raw.layout {
        raw::Layout::Mosaic(cfa) => {
            let (fw, fh) = (raw.width, raw.height);
            let mut cfa_plane = Plane::new(fw, fh);
            for y in 0..fh {
                for x in 0..fw {
                    let c = cfa.color_at(y, x).min(2);
                    cfa_plane.d[y * fw + x] = raw.normalised(x, y, 0).clamp(0.0, 1.0) * wb[c];
                }
            }
            // The pattern is looked up at absolute sensor positions, so the
            // usable area may start anywhere without swapping red and blue.
            demosaic_any(&cfa_plane, cfa, x0, y0, w, h)
        }
        raw::Layout::Linear => {
            // Already three values a pixel, but still in the camera's own
            // colours and still unbalanced.
            let mut img = Image::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    for c in 0..3 {
                        img.d[(y * w + x) * 3 + c] =
                            raw.normalised(x0 + x, y0 + y, c).clamp(0.0, 1.0) * wb[c];
                    }
                }
            }
            img
        }
    };
    reconstruct_highlights(&mut cam, &wb);

    // --- camera RGB -> linear sRGB ---------------------------------------
    // Not clamped.
    let m = cam_to_srgb(&scene_matrix(&raw.matrices, &raw.wb));
    // What is not kept is a pixel with no light in it.
    for px in cam.d.chunks_exact_mut(3) {
        let (r, g, b) = (px[0], px[1], px[2]);
        let out = [m[0][0] * r + m[0][1] * g + m[0][2] * b,
                   m[1][0] * r + m[1][1] * g + m[1][2] * b,
                   m[2][0] * r + m[2][1] * g + m[2][2] * b];
        if crate::ops::luminance_px(&out) > 0.0 {
            px.copy_from_slice(&out);
        } else {
            px.fill(0.0);
        }
    }

    // Not every decoder reads the orientation (rawler's ARW one does not), and
    // a portrait frame that comes up on its side is the first thing anyone
    // notices.
    let orientation = match raw.orientation {
        Orientation::Normal | Orientation::Unknown => Orientation::from_u16(meta.orientation),
        o => o,
    };
    Decoded { img: orient(&cam, orientation), meta }
}

/// A small picture of the file for a filmstrip, from the JPEG the camera
/// embedded rather than from the raw data.
pub fn thumbnail(bytes: &[u8], long_edge: usize) -> Result<Image, DecodeError> {
    use rawler::decoders::RawDecodeParams;
    let source = rawler::rawsource::RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&source)
        .map_err(|e| DecodeError::undecodable(e.to_string()))?;
    let params = RawDecodeParams::default();
    let picture = decoder.thumbnail_image(&source, &params).ok().flatten()
        .or_else(|| decoder.preview_image(&source, &params).ok().flatten())
        .ok_or_else(|| DecodeError { code: "no_preview", message: "no embedded preview".into(),
                                     make: String::new(), model: String::new() })?
        .to_rgb8();
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    let mut img = Image::new(w, h);
    for (i, v) in picture.as_raw().iter().enumerate() {
        img.d[i] = crate::ops::srgb_decode_scalar(*v as f32 / 255.0);
    }
    let small = crate::ops::thumbnail(&img, long_edge);
    let meta = read_exif(bytes);
    Ok(orient(&small, Orientation::from_u16(meta.orientation)))
}

/// The camera's XYZ matrix for the light this frame was taken in.
pub fn scene_matrix(matrices: &[(f32, [[f32; 3]; 3])], wb: &[f32; 3]) -> [[f32; 3]; 3] {
    match scene_temperature(matrices, wb) {
        Some(kelvin) => matrix_at(matrices, kelvin),
        None => matrices.first().map(|m| m.1).unwrap_or(SRGB_AS_CAMERA),
    }
}

/// The blend of the calibration matrices for a light of this temperature.
fn matrix_at(matrices: &[(f32, [[f32; 3]; 3])], kelvin: f32) -> [[f32; 3]; 3] {
    let (lo, hi) = (&matrices[0], &matrices[matrices.len() - 1]);
    let k = kelvin.clamp(lo.0, hi.0);
    let t = if (hi.0 - lo.0).abs() < 1.0 { 0.0 }
            else { (1.0 / k - 1.0 / lo.0) / (1.0 / hi.0 - 1.0 / lo.0) };
    let mut m = [[0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = lo.1[i][j] * (1.0 - t) + hi.1[i][j] * t;
        }
    }
    m
}

/// The colour temperature the camera's white balance was set for, as far as the
/// calibration matrices can tell, or `None` when there are fewer than two of
/// them to tell between.
pub fn scene_temperature(matrices: &[(f32, [[f32; 3]; 3])], wb: &[f32; 3]) -> Option<f32> {
    if matrices.len() < 2 {
        return None;
    }
    let (lo, hi) = (matrices[0].0, matrices[matrices.len() - 1].0);
    // What the camera recorded for a neutral surface: the reciprocal of the
    // multipliers that made it neutral.
    let neutral = [1.0 / wb[0].max(1e-6), 1.0 / wb[1].max(1e-6), 1.0 / wb[2].max(1e-6)];
    let mut kelvin = 5000.0f32;
    for _ in 0..8 {
        let Some(inv) = invert3(&matrix_at(matrices, kelvin)) else { break };
        let xyz: Vec<f32> = (0..3).map(|i| (0..3).map(|j| inv[i][j] * neutral[j]).sum()).collect();
        let sum = xyz[0] + xyz[1] + xyz[2];
        if !(sum > 1e-9) {
            break;
        }
        let next = mccamy(xyz[0] / sum, xyz[1] / sum).clamp(lo, hi);
        if !next.is_finite() {
            break;
        }
        let settled = (next - kelvin).abs() < 1.0;
        kelvin = next;
        if settled {
            break;
        }
    }
    Some(kelvin)
}

/// Correlated colour temperature from a chromaticity.
fn mccamy(x: f32, y: f32) -> f32 {
    let n = (x - 0.3320) / (0.1858 - y);
    449.0 * n * n * n + 3525.0 * n * n + 6823.3 * n + 5520.33
}

/// A camera that describes no matrix at all: treat its channels as sRGB's,
/// which is wrong, but wrong in the same way as having no profile anywhere.
const SRGB_AS_CAMERA: [[f32; 3]; 3] = [
    [3.2404542, -1.5371385, -0.4985314],
    [-0.9692660, 1.8760108, 0.0415560],
    [0.0556434, -0.2040259, 1.0572252],
];

/// One cell of the hue field per this many pixels each way.
const HUE_SCALE: usize = 4;

/// What colour the neighbourhood is, everywhere, including where the sensor
/// stopped recording it.
struct HueField {
    /// Three proportions per cell, summing to one.
    d: Vec<f32>,
    /// How much of each cell is a real measurement rather than filled in.
    wt: Vec<f32>,
    w: usize,
    h: usize,
}

impl HueField {
    fn at(&self, x: usize, y: usize) -> [f32; 3] {
        let u = (x as f32 + 0.5) / HUE_SCALE as f32 - 0.5;
        let v = (y as f32 + 0.5) / HUE_SCALE as f32 - 0.5;
        sample_field(&self.d, &self.wt, self.w, self.h, u, v).0
    }
}

/// A bilinear read of a field, weighted so that a cell nothing is known about
/// contributes nothing rather than contributing black.
fn sample_field(d: &[f32], wt: &[f32], w: usize, h: usize, u: f32, v: f32) -> ([f32; 3], f32) {
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let mut acc = [0f32; 3];
    let mut acc_w = 0.0f32;
    for (dy, wy) in [(0i32, 1.0 - fy), (1, fy)] {
        for (dx, wx) in [(0i32, 1.0 - fx), (1, fx)] {
            let xi = (x0 as i32 + dx).clamp(0, w as i32 - 1) as usize;
            let yi = (y0 as i32 + dy).clamp(0, h as i32 - 1) as usize;
            let j = yi * w + xi;
            let k = wx * wy * wt[j];
            if k <= 0.0 {
                continue;
            }
            for c in 0..3 {
                acc[c] += d[j * 3 + c] * k;
            }
            acc_w += k;
        }
    }
    if acc_w > 0.0 {
        for c in 0..3 {
            acc[c] /= acc_w;
        }
    }
    (acc, acc_w)
}

/// The colour around every point, with the holes the highlights punched in it
/// filled in from their edges.
fn surrounding_hue(img: &Image, onset: &[f32; 3], limit: &[f32; 3]) -> Option<HueField> {
    let (w, h) = (img.w.div_ceil(HUE_SCALE), img.h.div_ceil(HUE_SCALE));
    let mut d = vec![0f32; w * h * 3];
    let mut wt = vec![0f32; w * h];

    // Every other pixel each way.
    for y in (0..img.h).step_by(2) {
        for x in (0..img.w).step_by(2) {
            let i = (y * img.w + x) * 3;
            let px = &img.d[i..i + 3];
            // Nowhere near any ceiling is the overwhelmingly common case, and
            // three smoothsteps twenty million times is worth not doing.
            let clear = px[0] < onset[0] && px[1] < onset[1] && px[2] < onset[2];
            // Only a pixel with every channel still in range describes a
            // colour.
            let known = if clear {
                1.0
            } else {
                let blown = blown_at(px, onset, limit);
                1.0 - blown[0].max(blown[1]).max(blown[2])
            };
            let sum = px[0] + px[1] + px[2];
            if known <= 0.0 || sum <= 1e-6 {
                continue;
            }
            let j = (y / HUE_SCALE) * w + x / HUE_SCALE;
            for c in 0..3 {
                d[j * 3 + c] += known * px[c] / sum;
            }
            wt[j] += known;
        }
    }
    for j in 0..w * h {
        if wt[j] > 0.0 {
            for c in 0..3 {
                d[j * 3 + c] /= wt[j];
            }
            wt[j] = 1.0;
        }
    }
    if wt.iter().all(|v| *v <= 0.0) {
        return None;
    }

    // Coarsen until the largest hole has closed over.
    let mut levels = vec![(d, wt, w, h)];
    while levels.last().map_or(false, |(_, _, lw, lh)| *lw > 1 || *lh > 1) {
        let (pd, pwt, lw, lh) = levels.last().unwrap();
        let (nw, nh) = (lw.div_ceil(2), lh.div_ceil(2));
        let mut nd = vec![0f32; nw * nh * 3];
        let mut nwt = vec![0f32; nw * nh];
        for y in 0..*lh {
            for x in 0..*lw {
                let i = y * lw + x;
                if pwt[i] <= 0.0 {
                    continue;
                }
                let j = (y / 2) * nw + x / 2;
                for c in 0..3 {
                    nd[j * 3 + c] += pd[i * 3 + c] * pwt[i];
                }
                nwt[j] += pwt[i];
            }
        }
        for j in 0..nw * nh {
            if nwt[j] > 0.0 {
                for c in 0..3 {
                    nd[j * 3 + c] /= nwt[j];
                }
                nwt[j] = nwt[j].min(1.0);
            }
        }
        levels.push((nd, nwt, nw, nh));
    }

    // Refine back down, keeping what was measured and taking the rest from the
    // level above.
    for k in (0..levels.len() - 1).rev() {
        let (cd, cwt, cw, ch) = levels[k + 1].clone();
        let (fd, fwt, fw, fh) = &mut levels[k];
        for y in 0..*fh {
            for x in 0..*fw {
                let i = y * *fw + x;
                let here = fwt[i];
                if here >= 1.0 {
                    continue;
                }
                let u = (x as f32 - 0.5) * 0.5;
                let v = (y as f32 - 0.5) * 0.5;
                let (above, above_w) = sample_field(&cd, &cwt, cw, ch, u, v);
                if above_w <= 0.0 {
                    continue;
                }
                for c in 0..3 {
                    fd[i * 3 + c] = fd[i * 3 + c] * here + above[c] * (1.0 - here);
                }
                fwt[i] = here + above_w * (1.0 - here);
            }
        }
    }

    let (d, wt, w, h) = levels.swap_remove(0);
    Some(HueField { d, wt, w, h })
}

/// How far each channel has run out of range.
fn blown_at(px: &[f32], onset: &[f32; 3], limit: &[f32; 3]) -> [f32; 3] {
    [
        smoothstep(onset[0], limit[0], px[0]),
        smoothstep(onset[1], limit[1], px[1]),
        smoothstep(onset[2], limit[2], px[2]),
    ]
}

/// Recover blown highlights from what surrounds them.
fn reconstruct_highlights(img: &mut Image, wb: &[f32; 3]) {
    // Where recovery starts, as a fraction of each channel's own ceiling.
    const ONSET: f32 = 0.96;

    // Each channel's ceiling is where the white balance put its saturation
    // point, and these do not vary per pixel, so work them out once.
    let limit = [wb[0].max(1e-6), wb[1].max(1e-6), wb[2].max(1e-6)];
    let onset = [ONSET * limit[0], ONSET * limit[1], ONSET * limit[2]];

    let Some(hues) = surrounding_hue(img, &onset, &limit) else {
        // Not one pixel in the frame kept all three channels, so there is
        // nothing to carry inward and every highlight is on its own.
        for px in img.d.chunks_exact_mut(3) {
            let blown = blown_at(px, &onset, &limit);
            if blown[0] + blown[1] + blown[2] <= 0.0 {
                continue;
            }
            let hi = px[0].max(px[1]).max(px[2]);
            for c in 0..3 {
                px[c] += blown[c] * (hi - px[c]);
            }
        }
        return;
    };

    for y in 0..img.h {
        for x in 0..img.w {
            let i = (y * img.w + x) * 3;
            let px = [img.d[i], img.d[i + 1], img.d[i + 2]];
            // Comparisons before smoothsteps: almost every pixel in a frame is
            // nowhere near a ceiling and can be dismissed in three loads.
            if px[0] < onset[0] && px[1] < onset[1] && px[2] < onset[2] {
                continue;
            }
            let blown = blown_at(&px, &onset, &limit);
            if blown[0] + blown[1] + blown[2] <= 0.0 {
                continue;
            }

            let mut v = px;

            // One channel gone leaves two that still describe the colour, and
            // green is the one worth rebuilding because it sits between the
            // others in wavelength.
            let mid = 0.5 * (v[0] + v[2]);
            v[1] += blown[1] * (mid - v[1]).max(0.0);

            // How far gone the pixel is overall.
            let mut order = blown;
            order.sort_by(|a, b| b.partial_cmp(a).unwrap());
            let core = order[1];
            if core <= 0.0 {
                img.d[i..i + 3].copy_from_slice(&v);
                continue;
            }

            // Here is where the surroundings earn their keep.
            let hue = hues.at(x, y);
            let (mut scale, mut anchor) = (0.0f32, 0.0f32);
            for c in 0..3 {
                let known = 1.0 - blown[c];
                if known > 0.0 && hue[c] > 1e-4 {
                    scale += known * v[c] / hue[c];
                    anchor += known;
                }
            }
            let hi = v[0].max(v[1]).max(v[2]);
            let scale = if anchor > 1e-4 { scale / anchor } else { 0.0 };
            // Brightness washes colour out, so the hue at the rim of a blown
            // sun is the most saturated it ever gets and the core really is
            // white.
            let trust = (0.5 * anchor).clamp(0.0, 1.0);
            for c in 0..3 {
                // Bounded at both ends: below by where it already is, which is
                // its ceiling, and above by the brightest channel the pixel
                // still has.
                let from_neighbours = (scale * hue[c]).clamp(v[c], hi);
                let want = from_neighbours * trust + hi * (1.0 - trust);
                v[c] += core * (want - v[c]);
            }
            img.d[i..i + 3].copy_from_slice(&v);
        }
    }
}

/// dcraw's `cam_xyz_coeff`: compose with the sRGB primaries, normalise each
/// row to unit sum (so neutral camera values stay neutral), then invert.
fn cam_to_srgb(xyz_to_cam: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut cam_rgb = [[0f32; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += xyz_to_cam[i][k] * XYZ_RGB[k][j];
            }
            cam_rgb[i][j] = s;
        }
    }
    for i in 0..3 {
        let num: f32 = cam_rgb[i].iter().sum();
        if num.abs() > 1e-9 {
            for j in 0..3 {
                cam_rgb[i][j] /= num;
            }
        }
    }
    invert3(&cam_rgb).unwrap_or([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
}

fn invert3(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let id = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * id,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * id,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * id,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * id,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * id,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * id,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * id,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * id,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * id,
        ],
    ])
}

// -------------------------------------------------------------------------
// demosaic
// -------------------------------------------------------------------------
// Two passes.

/// Interpolate a Bayer mosaic to full colour, choosing a direction for green
/// and reconstructing red and blue as differences from it.
pub fn demosaic_directional(cfa: &Plane, pattern: &CFA, x0: usize, y0: usize,
                            w: usize, h: usize) -> Image {
    // Everything below assumes the four-site Bayer quad.
    if pattern.width != 2 || pattern.height != 2 {
        return demosaic_any(cfa, pattern, x0, y0, w, h);
    }
    let mut out = Image::new(w, h);

    // The pattern repeats every two rows and columns, so resolve it once for
    // the crop's phase rather than paying color_at's modulo indexing on every
    // one of the dozen-odd reads each pixel makes.
    let phase = [
        [pattern.color_at(y0, x0), pattern.color_at(y0, x0 + 1)],
        [pattern.color_at(y0 + 1, x0), pattern.color_at(y0 + 1, x0 + 1)],
    ];
    let colour = |x: usize, y: usize| phase[y & 1][x & 1];
    let is_green = |c: usize| c == 1 || c == 3;

    // Out-of-frame neighbours are reflected, not clamped.
    let reflect = |v: i64, n: usize| -> usize {
        let n = n as i64;
        let folded = if v < 0 { -v } else if v >= n { 2 * (n - 1) - v } else { v };
        // The guard is for a frame narrower than the neighbourhood, where one
        // fold is not enough to land inside.
        folded.clamp(0, n - 1) as usize
    };
    // Reflection happens in crop coordinates so that the second pass, which
    // reads a sensor value and a reconstructed green at what must be the same
    // pixel, cannot have the two disagree about where the edge is.
    let row = |dy: i64, y: usize| -> usize { reflect(y as i64 + dy, h) };
    let col = |dx: i64, x: usize| -> usize { reflect(x as i64 + dx, w) };
    let sensor_row = |r: usize| -> usize { (y0 + r) * cfa.w };
    let sensor_col = |c: usize| -> usize { x0 + c };

    // --- pass 1: green at every pixel -----------------------------------
    // At a red or blue site green has four neighbours, two horizontal and two
    // vertical.
    for y in 0..h {
        let (rm2, rm1, r0, rp1, rp2) = (
            sensor_row(row(-2, y)), sensor_row(row(-1, y)), sensor_row(row(0, y)),
            sensor_row(row(1, y)), sensor_row(row(2, y)),
        );
        for x in 0..w {
            let i = (y * w + x) * 3;
            let (cm2, cm1, c0, cp1, cp2) = (
                sensor_col(col(-2, x)), sensor_col(col(-1, x)), sensor_col(col(0, x)),
                sensor_col(col(1, x)), sensor_col(col(2, x)),
            );
            let centre = cfa.d[r0 + c0];
            if is_green(colour(x, y)) {
                out.d[i + 1] = centre;
                continue;
            }
            let (n1, s1) = (cfa.d[rm1 + c0], cfa.d[rp1 + c0]);
            let (w1, e1) = (cfa.d[r0 + cm1], cfa.d[r0 + cp1]);
            let (n2, s2) = (cfa.d[rm2 + c0], cfa.d[rp2 + c0]);
            let (w2, e2) = (cfa.d[r0 + cm2], cfa.d[r0 + cp2]);

            let curve_h = 2.0 * centre - w2 - e2;
            let curve_v = 2.0 * centre - n2 - s2;
            let g_h = 0.5 * (w1 + e1) + 0.25 * curve_h;
            let g_v = 0.5 * (n1 + s1) + 0.25 * curve_v;

            // How much the image is doing in each direction: the green
            // difference across the centre plus the same-colour curvature.
            let d_h = (w1 - e1).abs() + curve_h.abs();
            let d_v = (n1 - s1).abs() + curve_v.abs();

            // Weight each estimate by the *other* direction's activity, so the
            // flatter direction wins.
            let total = d_h + d_v;
            out.d[i + 1] = if total > 0.0 {
                // How lopsided the two directions are, -1 (all the activity
                // vertical, so interpolate horizontally) to +1.
                let lean = (d_v - d_h) / total;
                // Squaring the lean while keeping its sign holds the estimate
                // near the isotropic average until one direction is clearly
                // flatter, and still commits fully when it is.
                let weight_h = 0.5 * (1.0 + lean * lean.abs());
                g_h * weight_h + g_v * (1.0 - weight_h)
            } else {
                // Flat both ways, so the two estimates agree anyway.
                0.5 * (g_h + g_v)
            };
        }
    }

    // --- pass 2: red and blue from green plus a colour difference --------
    // Interpolating red directly would carry every edge in the scene into the
    // estimate, and since red is sampled at half green's density it cannot
    // follow those edges.
    for y in 0..h {
        let (rm1, r0, rp1) = (row(-1, y), row(0, y), row(1, y));
        for x in 0..w {
            let i = (y * w + x) * 3;
            let (cm1, c0, cp1) = (col(-1, x), col(0, x), col(1, x));
            let g = out.d[(r0 * w + c0) * 3 + 1];
            // A sensor reading minus the green reconstructed at the same pixel,
            // both addressed from one pair of crop coordinates so they cannot
            // drift apart at the frame edge.
            let diff = |r: usize, c: usize| -> f32 {
                cfa.d[sensor_row(r) + sensor_col(c)] - out.d[(r * w + c) * 3 + 1]
            };
            let (r, b);
            if is_green(colour(x, y)) {
                // A green site has red on one axis and blue on the other.
                let across = 0.5 * (diff(r0, cm1) + diff(r0, cp1));
                let along = 0.5 * (diff(rm1, c0) + diff(rp1, c0));
                if colour(x + 1, y) == 0 {
                    r = g + across;
                    b = g + along;
                } else {
                    r = g + along;
                    b = g + across;
                }
            } else {
                // A red site has blue only on the diagonals, and vice versa.
                let other = g
                    + 0.25
                        * (diff(rm1, cm1) + diff(rm1, cp1)
                            + diff(rp1, cm1) + diff(rp1, cp1));
                let centre = cfa.d[sensor_row(r0) + sensor_col(c0)];
                if colour(x, y) == 0 {
                    r = centre;
                    b = other;
                } else {
                    b = centre;
                    r = other;
                }
            }
            out.d[i] = r.max(0.0);
            out.d[i + 2] = b.max(0.0);
        }
    }
    out
}

/// The demosaic for whatever pattern the sensor has.
pub fn demosaic_any(cfa: &Plane, pattern: &CFA, x0: usize, y0: usize, w: usize, h: usize) -> Image {
    if pattern.width == 2 && pattern.height == 2 {
        demosaic(cfa, pattern, x0, y0, w, h)
    } else {
        demosaic_pattern(cfa, pattern, x0, y0, w, h)
    }
}

/// Demosaic any RGB pattern, by colour difference.
pub fn demosaic_pattern(cfa: &Plane, pattern: &CFA, x0: usize, y0: usize, w: usize, h: usize) -> Image {
    let colour = |x: i64, y: i64| -> usize {
        pattern.color_at((y0 as i64 + y).max(0) as usize, (x0 as i64 + x).max(0) as usize).min(2)
    };
    let inside = |x: i64, y: i64| -> bool {
        let (xx, yy) = (x0 as i64 + x, y0 as i64 + y);
        xx >= 0 && yy >= 0 && (xx as usize) < cfa.w && (yy as usize) < cfa.h
    };
    let at = |x: i64, y: i64| -> f32 {
        cfa.d[(y0 as i64 + y) as usize * cfa.w + (x0 as i64 + x) as usize]
    };

    // --- green everywhere ---------------------------------------------------
    let mut green = Plane::new(w, h);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = y as usize * w + x as usize;
            if colour(x, y) == 1 {
                green.d[i] = at(x, y);
                continue;
            }
            let pair = |dx: i64, dy: i64| -> Option<(f32, f32)> {
                let (ax, ay, bx, by) = (x - dx, y - dy, x + dx, y + dy);
                if inside(ax, ay) && inside(bx, by) && colour(ax, ay) == 1 && colour(bx, by) == 1 {
                    Some((at(ax, ay), at(bx, by)))
                } else {
                    None
                }
            };
            let lines = [pair(1, 0), pair(0, 1)];
            let best = lines.iter().flatten().min_by(|a, b| (a.0 - a.1).abs().total_cmp(&(b.0 - b.1).abs()));
            green.d[i] = match best {
                Some((a, b)) => 0.5 * (a + b),
                None => {
                    let (mut sum, mut n) = (0.0f32, 0.0f32);
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if (dx, dy) != (0, 0) && inside(x + dx, y + dy) && colour(x + dx, y + dy) == 1 {
                                let wt = if dx == 0 || dy == 0 { 1.0 } else { 0.5 };
                                sum += wt * at(x + dx, y + dy);
                                n += wt;
                            }
                        }
                    }
                    if n > 0.0 { sum / n } else { at(x, y) }
                }
            };
        }
    }

    // --- red and blue as a difference from green ---------------------------
    let mut out = Image::new(w, h);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let i = y as usize * w + x as usize;
            let g = green.d[i];
            let here = colour(x, y);
            let mut px = [0f32; 3];
            px[1] = g;
            for c in [0usize, 2] {
                if here == c {
                    px[c] = at(x, y);
                    continue;
                }
                let (mut sum, mut n) = (0.0f32, 0.0f32);
                for dy in -2..=2i64 {
                    for dx in -2..=2i64 {
                        let (sx, sy) = (x + dx, y + dy);
                        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 || colour(sx, sy) != c {
                            continue;
                        }
                        let wt = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
                        sum += wt * (at(sx, sy) - green.d[sy as usize * w + sx as usize]);
                        n += wt;
                    }
                }
                px[c] = if n > 0.0 { g + sum / n } else { g };
            }
            for c in 0..3 {
                out.d[i * 3 + c] = px[c].max(0.0);
            }
        }
    }
    out
}

/// Gradient-corrected bilinear interpolation (Malvar, He & Cutler 2004).
pub fn demosaic(cfa: &Plane, pattern: &CFA, x0: usize, y0: usize, w: usize,
                h: usize) -> Image {
    let mut out = Image::new(w, h);
    let at = |x: i64, y: i64| -> f32 {
        let xx = (x0 as i64 + x).clamp(0, cfa.w as i64 - 1) as usize;
        let yy = (y0 as i64 + y).clamp(0, cfa.h as i64 - 1) as usize;
        cfa.d[yy * cfa.w + xx]
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let c = pattern.color_at((y0 as i64 + y) as usize, (x0 as i64 + x) as usize);
            let centre = at(x, y);
            // neighbourhood
            let n1 = at(x, y - 1);
            let s1 = at(x, y + 1);
            let w1 = at(x - 1, y);
            let e1 = at(x + 1, y);
            let n2 = at(x, y - 2);
            let s2 = at(x, y + 2);
            let w2 = at(x - 2, y);
            let e2 = at(x + 2, y);
            let nw = at(x - 1, y - 1);
            let ne = at(x + 1, y - 1);
            let sw = at(x - 1, y + 1);
            let se = at(x + 1, y + 1);

            let cross = n1 + s1 + w1 + e1;
            let diag = nw + ne + sw + se;
            let far = n2 + s2 + w2 + e2;
            let far_h = w2 + e2;
            let far_v = n2 + s2;

            let (r, g, b);
            if c == 1 || c == 3 {
                // Green site.  Which of R/B lies horizontally decides the two
                // asymmetric kernels.
                let row_has_red = pattern.color_at((y0 as i64 + y) as usize, (x0 as i64 + x + 1) as usize) == 0;
                let g_v = centre;
                let hori = (5.0 * centre + 4.0 * (w1 + e1) - (nw + ne + sw + se) - far_h
                    + 0.5 * far_v)
                    / 8.0;
                let vert = (5.0 * centre + 4.0 * (n1 + s1) - (nw + ne + sw + se) - far_v
                    + 0.5 * far_h)
                    / 8.0;
                if row_has_red {
                    r = hori;
                    b = vert;
                } else {
                    r = vert;
                    b = hori;
                }
                g = g_v;
            } else {
                // Red or blue site.
                let g_est = (4.0 * centre + 2.0 * cross - far) / 8.0;
                let other = (6.0 * centre + 2.0 * diag - 1.5 * far) / 8.0;
                if c == 0 {
                    r = centre;
                    g = g_est;
                    b = other;
                } else {
                    b = centre;
                    g = g_est;
                    r = other;
                }
            }
            let i = (y as usize * w + x as usize) * 3;
            out.d[i] = r.max(0.0);
            out.d[i + 1] = g.max(0.0);
            out.d[i + 2] = b.max(0.0);
        }
    }
    out
}

// -------------------------------------------------------------------------
// orientation
// -------------------------------------------------------------------------

fn orient(img: &Image, o: Orientation) -> Image {
    use Orientation::*;
    match o {
        Normal | Unknown => img.clone(),
        HorizontalFlip => map(img, img.w, img.h, |x, y, w, _| (w - 1 - x, y)),
        Rotate180 => map(img, img.w, img.h, |x, y, w, h| (w - 1 - x, h - 1 - y)),
        VerticalFlip => map(img, img.w, img.h, |x, y, _, h| (x, h - 1 - y)),
        Transpose => map(img, img.h, img.w, |x, y, _, _| (y, x)),
        Rotate90 => map(img, img.h, img.w, |x, y, _, h| (y, h - 1 - x)),
        Transverse => map(img, img.h, img.w, |x, y, w, h| (w - 1 - y, h - 1 - x)),
        Rotate270 => map(img, img.h, img.w, |x, y, w, _| (w - 1 - y, x)),
    }
}

/// `f` maps a destination pixel to a source pixel, given the *source* size.
fn map<F: Fn(usize, usize, usize, usize) -> (usize, usize)>(
    img: &Image,
    ow: usize,
    oh: usize,
    f: F,
) -> Image {
    let mut out = Image::new(ow, oh);
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = f(x, y, img.w, img.h);
            let si = (sy.min(img.h - 1) * img.w + sx.min(img.w - 1)) * 3;
            let di = (y * ow + x) * 3;
            out.d[di..di + 3].copy_from_slice(&img.d[si..si + 3]);
        }
    }
    out
}

// -------------------------------------------------------------------------
// EXIF
// -------------------------------------------------------------------------

fn read_exif(bytes: &[u8]) -> Meta {
    let mut m = Meta {
        iso: 100.0,
        aperture: 8.0,
        focal_mm: 50.0,
        focal35_mm: 0.0,
        shutter_s: 1.0 / 125.0,
        ..Default::default()
    };
    let reader = exif::Reader::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = match reader.read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return m,
    };
    use exif::{In, Tag};
    let text = |tag: Tag| -> String {
        exif.get_field(tag, In::PRIMARY)
            .map(|f| f.display_value().to_string().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let num = |tag: Tag| -> Option<f32> {
        exif.get_field(tag, In::PRIMARY).and_then(|f| match &f.value {
            exif::Value::Rational(v) if !v.is_empty() => Some(v[0].to_f32()),
            exif::Value::SRational(v) if !v.is_empty() => Some(v[0].to_f32()),
            exif::Value::Short(v) if !v.is_empty() => Some(v[0] as f32),
            exif::Value::Long(v) if !v.is_empty() => Some(v[0] as f32),
            exif::Value::Float(v) if !v.is_empty() => Some(v[0]),
            exif::Value::Double(v) if !v.is_empty() => Some(v[0] as f32),
            _ => None,
        })
    };
    m.make = text(Tag::Make);
    m.model = text(Tag::Model);
    m.lens_model = text(Tag::LensModel);
    if let Some(v) = num(Tag::PhotographicSensitivity) {
        m.iso = v;
    }
    if let Some(v) = num(Tag::FNumber) {
        m.aperture = v;
    }
    if let Some(v) = num(Tag::FocalLength) {
        m.focal_mm = v;
    }
    if let Some(v) = num(Tag::FocalLengthIn35mmFilm) {
        m.focal35_mm = v;
    }
    if let Some(v) = num(Tag::ExposureTime) {
        m.shutter_s = v;
    }
    if let Some(v) = num(Tag::ExposureBiasValue) {
        m.exposure_comp = v;
    }
    if let Some(v) = num(Tag::Flash) {
        m.flash = (v as i32) % 2 == 1;
    }
    if let Some(v) = num(Tag::Orientation) {
        m.orientation = v as u16;
    }
    // DateTimeOriginal is when the photograph was taken.
    for tag in [Tag::DateTimeOriginal, Tag::DateTime] {
        if let Some(field) = exif.get_field(tag, In::PRIMARY) {
            if let exif::Value::Ascii(ref v) = field.value {
                if let Some(when) = v.first().and_then(|a| exif::DateTime::from_ascii(a).ok()) {
                    m.captured_s = epoch_seconds(&when);
                    break;
                }
            }
        }
    }
    m
}

/// A civil date and time as seconds from 1970, ignoring leap seconds and time
/// zones alike.
fn epoch_seconds(t: &exif::DateTime) -> i64 {
    let (y, m, d) = (t.year as i64, t.month as i64, t.day as i64);
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + t.hour as i64 * 3_600 + t.minute as i64 * 60 + t.second as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capture time is the only thing that tells two frames of one burst
    /// apart from two photographs of different things, so the arithmetic that
    /// produces it is worth pinning to known answers.
    #[test]
    fn a_civil_date_becomes_the_right_instant() {
        let at = |y, mo, d, h, mi, s| epoch_seconds(&::exif::DateTime {
            year: y, month: mo, day: d, hour: h, minute: mi, second: s,
            nanosecond: None, offset: None,
        });
        assert_eq!(at(1970, 1, 1, 0, 0, 0), 0, "the epoch itself");
        assert_eq!(at(2000, 1, 1, 0, 0, 0), 946_684_800, "a century boundary that is a leap year");
        assert_eq!(at(2024, 2, 29, 12, 0, 0), 1_709_208_000, "a leap day");
        assert_eq!(at(2026, 9, 22, 9, 7, 40), 1_790_068_060, "the sample bracket");
        // What the grouping actually reads: the gap between two frames.
        assert_eq!(at(2026, 9, 22, 9, 7, 41) - at(2026, 9, 22, 9, 7, 40), 1);
        assert_eq!(at(2026, 9, 12, 19, 59, 16) - at(2026, 9, 12, 19, 50, 51), 505);
    }

    /// A highlight that clipped in every channel must come out neutral, not
    /// tinted by the inverse of the white balance.
    #[test]
    fn blown_highlights_go_white() {
        let wb = [1.0f32, 0.37, 0.52];
        let mut img = Image::new(3, 1);
        // fully clipped: each channel sitting on its own ceiling
        img.d[0..3].copy_from_slice(&[1.0, 0.37, 0.52]);
        // a genuinely red subject: only red is at its limit
        img.d[3..6].copy_from_slice(&[1.0, 0.05, 0.04]);
        // an ordinary midtone, nowhere near any ceiling
        img.d[6..9].copy_from_slice(&[0.30, 0.12, 0.18]);
        reconstruct_highlights(&mut img, &wb);

        let spread = |px: &[f32]| {
            px[0].max(px[1]).max(px[2]) / px[0].min(px[1]).min(px[2]).max(1e-6)
        };
        assert!(spread(&img.d[0..3]) < 1.02, "clipped pixel still tinted: {:?}", &img.d[0..3]);
        assert!(img.d[0] > 0.98, "clipped pixel should be white: {:?}", &img.d[0..3]);
        // the red subject keeps its colour
        assert!(img.d[4] < 0.1 && img.d[5] < 0.1, "red subject was neutralised: {:?}", &img.d[3..6]);
        // and an ordinary pixel is untouched
        assert_eq!(&img.d[6..9], &[0.30, 0.12, 0.18]);
    }

    /// A blown region has to keep the colour of what is around it rather than
    /// going grey in the middle.
    #[test]
    fn a_blown_region_keeps_the_colour_around_it() {
        let wb = [1.0f32, 0.36, 0.55];
        let (w, h) = (64usize, 64usize);
        let mut img = Image::new(w, h);
        // A warm sky, every channel comfortably inside its own ceiling.
        let sky = [0.30f32, 0.20, 0.12];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 3;
                img.d[i..i + 3].copy_from_slice(&sky);
                let (dx, dy) = (x as f32 - 32.0, y as f32 - 24.0);
                if dx * dx + dy * dy < 12.0 * 12.0 {
                    // Green and blue sitting on their ceilings, red still
                    // climbing: two channels gone and one measurement left.
                    img.d[i..i + 3].copy_from_slice(&[0.80, wb[1], wb[2]]);
                }
            }
        }
        reconstruct_highlights(&mut img, &wb);

        let middle = {
            let i = (24 * w + 32) * 3;
            [img.d[i], img.d[i + 1], img.d[i + 2]]
        };
        assert!(middle[1] > wb[1] && middle[2] > wb[2],
                "the lost channels should have been rebuilt, not left at their \
                 ceilings: {middle:?}");
        assert!(middle[0] > middle[1] && middle[0] > middle[2],
                "the warmth of the surrounding sky should have carried inward: {middle:?}");
        let spread = middle[0] / middle[1].min(middle[2]).max(1e-6);
        assert!(spread > 1.05, "the middle went neutral, which is the flat disc \
                 this exists to avoid: {middle:?}");
        // And the sky itself is none of this function's business.
        let corner = [img.d[0], img.d[1], img.d[2]];
        assert_eq!(corner, sky, "an unblown pixel was touched: {corner:?}");
    }

    /// With nothing unblown anywhere there is no colour to carry and no
    /// brightness to anchor, and neutral is the only honest answer.
    #[test]
    fn a_frame_with_nothing_left_goes_neutral() {
        let wb = [1.0f32, 0.36, 0.55];
        let mut img = Image::new(8, 8);
        for px in img.d.chunks_exact_mut(3) {
            px.copy_from_slice(&[1.0, wb[1], wb[2]]);
        }
        reconstruct_highlights(&mut img, &wb);
        for px in img.d.chunks_exact(3) {
            let spread = px[0].max(px[1]).max(px[2]) / px[0].min(px[1]).min(px[2]).max(1e-6);
            assert!(spread < 1.02, "a wholly blown frame should be white: {px:?}");
        }
    }

    /// Anything that is really a file offset has to be left behind: it points
    /// into the raw, and the export is not the raw.
    #[test]
    fn offsets_into_the_original_are_not_copied() {
        // A Sony ARW header carrying DNGPrivateData and a MakerNote, both of
        // which are positions in the file rather than facts about the picture.
        let entries = collect_exif(include_bytes!("testdata/offsets.exif"));
        for tag in [0xC634u16, 0x927C] {
            assert!(!entries.iter().any(|e| e.tag == tag),
                    "tag 0x{tag:04X} is a file offset and must not be copied");
        }
        // and the ordinary description of the photograph is still there
        assert!(entries.iter().any(|e| e.tag == 0x010F), "the make was lost");
    }

    /// Mosaic a known image and demosaic it back.
    fn round_trip(method: fn(&Plane, &CFA, usize, usize, usize, usize) -> Image,
                  truth: &Image, cfa: &CFA) -> Image {
        let mut plane = Plane::new(truth.w, truth.h);
        for y in 0..truth.h {
            for x in 0..truth.w {
                let c = cfa.color_at(y, x).min(2);
                plane.d[y * truth.w + x] = truth.d[(y * truth.w + x) * 3 + c];
            }
        }
        method(&plane, cfa, 0, 0, truth.w, truth.h)
    }

    /// A flat field has one right answer and every interpolator must find it.
    #[test]
    fn flat_field_survives_demosaic() {
        let cfa = CFA::new("RGGB");
        let (w, h) = (32usize, 24usize);
        let colour = [0.4f32, 0.55, 0.3];
        let mut truth = Image::new(w, h);
        for px in truth.d.chunks_exact_mut(3) {
            px.copy_from_slice(&colour);
        }
        for (name, method) in [("demosaic", demosaic as fn(&Plane, &CFA, usize, usize, usize, usize) -> Image),
                               ("directional", demosaic_directional)] {
            let got = round_trip(method, &truth, &cfa);
            // The border reads clamped neighbours, so judge the interior.
            for y in 2..h - 2 {
                for x in 2..w - 2 {
                    for c in 0..3 {
                        let v = got.d[(y * w + x) * 3 + c];
                        assert!((v - colour[c]).abs() < 1e-4,
                                "{name} shifted a flat field at {x},{y} channel {c}: \
                                 {v} against {}", colour[c]);
                    }
                }
            }
        }
    }

    /// The interpolated channels must actually track the sensor, not merely
    /// average it.
    #[test]
    fn horizontal_ramp_survives_demosaic() {
        let cfa = CFA::new("RGGB");
        let (w, h) = (48usize, 16usize);
        let mut truth = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.1 + 0.6 * (x as f32 / w as f32);
                for c in 0..3 {
                    truth.d[(y * w + x) * 3 + c] = v;
                }
            }
        }
        for (name, method) in [("demosaic", demosaic as fn(&Plane, &CFA, usize, usize, usize, usize) -> Image),
                               ("directional", demosaic_directional)] {
            let got = round_trip(method, &truth, &cfa);
            for y in 2..h - 2 {
                for x in 2..w - 2 {
                    let i = (y * w + x) * 3;
                    let want = truth.d[i];
                    for c in 0..3 {
                        assert!((got.d[i + c] - want).abs() < 2e-3,
                                "{name} lost a linear ramp at {x},{y} channel {c}");
                    }
                }
            }
        }
    }

    /// The directional interpolator only knows the Bayer quad, so it has to
    /// hand anything else to the one that does not care.
    #[test]
    fn non_bayer_falls_back() {
        let xtrans = CFA::new(XTRANS);
        let (w, h) = (24usize, 24usize);
        let mut plane = Plane::new(w, h);
        for (i, v) in plane.d.iter_mut().enumerate() {
            *v = (i % 17) as f32 / 17.0;
        }
        let fallback = demosaic_directional(&plane, &xtrans, 0, 0, w, h);
        let generic = demosaic_any(&plane, &xtrans, 0, 0, w, h);
        assert_eq!(fallback.d, generic.d,
                   "a non-Bayer pattern must go to the pattern demosaic untouched");
    }

    const XTRANS: &str = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG";

    /// An X-Trans sensor looking at a flat colour and at a grey ramp.
    #[test]
    fn xtrans_stays_grey_and_flat() {
        let cfa = CFA::new(XTRANS);
        let (w, h) = (48usize, 36usize);
        let mut flat = Image::new(w, h);
        let colour = [0.4f32, 0.55, 0.3];
        for px in flat.d.chunks_exact_mut(3) {
            px.copy_from_slice(&colour);
        }
        let got = round_trip(demosaic_any, &flat, &cfa);
        for (i, px) in got.d.chunks_exact(3).enumerate() {
            for c in 0..3 {
                assert!((px[c] - colour[c]).abs() < 1e-4,
                        "flat field shifted at {} channel {c}: {}", i, px[c]);
            }
        }
        let bayer = round_trip(demosaic, &flat, &cfa);
        let wrong = bayer.d.chunks_exact(3).map(|px| (0..3).map(|c| (px[c] - colour[c]).abs())
            .fold(0.0f32, f32::max)).fold(0.0f32, f32::max);
        assert!(wrong > 0.05, "the Bayer kernels were expected to fail on X-Trans: {wrong}");

        let mut ramp = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = 0.1 + 0.6 * ((x + y) as f32 / (w + h) as f32);
                for c in 0..3 {
                    ramp.d[(y * w + x) * 3 + c] = v;
                }
            }
        }
        let spread = |img: &Image| -> f32 {
            let mut worst = 0f32;
            for y in 3..h - 3 {
                for x in 3..w - 3 {
                    let px = &img.d[(y * w + x) * 3..(y * w + x) * 3 + 3];
                    worst = worst.max(px[0].max(px[1]).max(px[2]) - px[0].min(px[1]).min(px[2]));
                }
            }
            worst
        };
        let ours = spread(&round_trip(demosaic_any, &ramp, &cfa));
        assert!(ours < 0.01, "false colour on a grey X-Trans ramp: {ours}");
    }

    /// The same thing through the real Bayer path.
    #[test]
    fn blown_bayer_region_develops_to_white() {
        let wb = [1.0f32, 0.37, 0.52];
        let cfa = CFA::new("RGGB");
        let (w, h) = (96usize, 32usize);
        let grey = 0.2f32;

        // Three vertical bands: a blown neutral, a well-exposed neutral, and a
        // genuinely saturated red that happens to blow only its red channel.
        let mut plane = Plane::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let c = cfa.color_at(y, x).min(2);
                plane.d[y * w + x] = if x < 32 {
                    wb[c] // every photosite on its own ceiling
                } else if x < 64 {
                    grey // neutral: equal after the white balance, by definition
                } else {
                    [1.0, 0.05, 0.04][c] // red at its limit, the rest nowhere near
                };
            }
        }

        let mut img = demosaic(&plane, &cfa, 0, 0, w, h);
        let at = |img: &Image, x: usize| {
            let i = ((h / 2) * w + x) * 3;
            [img.d[i], img.d[i + 1], img.d[i + 2]]
        };
        let cast = |p: [f32; 3]| p[0].max(p[1]).max(p[2]) / p[0].min(p[1]).min(p[2]).max(1e-6);

        let blown_before = at(&img, 16);
        assert!(cast(blown_before) > 2.5,
                "the bug this fixes did not reproduce: {blown_before:?}");

        reconstruct_highlights(&mut img, &wb);
        let blown = at(&img, 16);
        let neutral = at(&img, 48);
        let red = at(&img, 80);

        assert!(cast(blown) < 1.02, "blown region still tinted: {blown:?}");
        assert!(blown.iter().all(|v| *v > 0.98), "blown region is not white: {blown:?}");
        assert!(neutral.iter().all(|v| (*v - grey).abs() < 1e-3),
                "a well-exposed neutral was altered: {neutral:?}");
        assert!(red[0] > 0.9 && red[1] < 0.1 && red[2] < 0.1,
                "a saturated red subject was neutralised: {red:?}");

        // White in the camera's own space is only white in the picture because
        // the matrix carries neutrals through unchanged.
        let sony = [[0.6912, -0.1503, -0.0645], [-0.4472, 1.2370, 0.2313],
                    [-0.0819, 0.1706, 0.5785]];
        let m = cam_to_srgb(&sony);
        let out = [m[0][0] + m[0][1] + m[0][2],
                   m[1][0] + m[1][1] + m[1][2],
                   m[2][0] + m[2][1] + m[2][2]];
        assert!(cast(out) < 1.01, "camera neutral drifted through the matrix: {out:?}");
    }

    /// A linear DNG is demosaiced but not developed.
    #[test]
    fn a_linear_raw_is_balanced_and_matrixed() {
        let d65 = [[0.6972f32, -0.2408, -0.06], [-0.433, 1.2101, 0.2515], [-0.0388, 0.1277, 0.5847]];
        let wb = [2.0f32, 1.0, 1.6];
        // A grey card as the sensor saw it: the reciprocal of the balance.
        let grey = [0.2 / wb[0], 0.2 / wb[1], 0.2 / wb[2]];
        let red = [0.3f32, 0.05, 0.02];
        let (w, h) = (8usize, 4usize);
        let mut data = Vec::new();
        for i in 0..w * h {
            let px = if i < w * h / 2 { grey } else { red };
            data.extend(px.iter().map(|v| 64.0 + v * (4095.0 - 64.0)));
        }
        let raw = raw::Raw::synthetic(w, h, data, raw::Layout::Linear, wb, vec![(6504.0, d65)],
                                      64.0, 4095.0);
        let out = develop_raw(&raw, Meta::default()).img;
        let g = &out.d[0..3];
        let cast = g.iter().cloned().fold(0.0f32, f32::max) / g.iter().cloned().fold(f32::MAX, f32::min).max(1e-6);
        assert!(cast < 1.02, "a grey card came out tinted: {g:?}");
        let r = &out.d[(w * h - 1) * 3..w * h * 3];
        let passthrough = [red[0] * wb[0], red[1] * wb[1], red[2] * wb[2]];
        let moved = (0..3).map(|c| (r[c] / r[1].max(1e-6) - passthrough[c] / passthrough[1]).abs())
            .fold(0.0f32, f32::max);
        assert!(moved > 0.1, "the matrix was skipped: {r:?} against {passthrough:?}");
    }

    /// Under daylight the daylight matrix, under a lamp the tungsten one.
    #[test]
    fn the_matrix_follows_the_light() {
        let d65 = [[0.6972f32, -0.2408, -0.06], [-0.433, 1.2101, 0.2515], [-0.0388, 0.1277, 0.5847]];
        let a = [[0.793f32, -0.406, 0.0417], [-0.374, 1.1119, 0.3015], [-0.0202, 0.0741, 0.6896]];
        let matrices = vec![(2856.0f32, a), (6504.0, d65)];
        let balance_under = |m: &[[f32; 3]; 3], white: [f32; 3]| -> [f32; 3] {
            let n: Vec<f32> = (0..3).map(|i| (0..3).map(|j| m[i][j] * white[j]).sum()).collect();
            [n[1] / n[0], 1.0, n[1] / n[2]]
        };
        let worst = |got: [[f32; 3]; 3], want: [[f32; 3]; 3]| -> f32 {
            (0..9).map(|k| (got[k / 3][k % 3] - want[k / 3][k % 3]).abs()).fold(0.0, f32::max)
        };
        let noon = scene_matrix(&matrices, &balance_under(&d65, [0.9505, 1.0, 1.0888]));
        assert!(worst(noon, d65) < 0.01, "daylight did not get the daylight matrix: {noon:?}");
        let lamp = scene_matrix(&matrices, &balance_under(&a, [1.0985, 1.0, 0.3558]));
        assert!(worst(lamp, a) < 0.01, "tungsten did not get the tungsten matrix: {lamp:?}");
        let one = scene_matrix(&matrices[1..], &[2.0, 1.0, 1.5]);
        assert_eq!(one, d65, "a single matrix must be used as it is");
    }
}

/// Everything in the original file worth carrying into the exported one.
pub fn collect_exif(bytes: &[u8]) -> Vec<crate::exif::Entry> {
    use crate::exif::{Entry, Ifd, Value};

    // Tags describing the raw's storage, or that we write ourselves.
    const SKIP_PRIMARY: &[u16] = &[
        // NewSubfileType and SubfileType.
        0x00FE, 0x00FF,
        0x0100, 0x0101, 0x0102, 0x0103, 0x0106, 0x0107, 0x0111, 0x0112, 0x0115, 0x0116,
        0x0117, 0x011C, 0x0118, 0x0119, 0x013D, 0x0142, 0x0143, 0x0144, 0x0145, 0x014A,
        0x0201, 0x0202, 0x828D, 0x828E, 0x8769, 0x8825, 0xC61A, 0xC61B, 0xC61C, 0xC61D,
        0xC61E, 0xC61F, 0xC620, 0xC621, 0xC622, 0xC623, 0xC624, 0xC625, 0xC626, 0xC627,
        // DNGPrivateData: four bytes that are an *offset* into the raw, where
        // the vendor's private block lives.
        0xC634,
    ];
    // MakerNote and the interoperability pointer: both position dependent.
    const SKIP_EXIF: &[u16] = &[0x927C, 0xA005, 0xA002, 0xA003];
    // A value larger than this is a thumbnail or a vendor blob, not metadata.
    const MAX_VALUE: usize = 64 * 1024;

    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let parsed = match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return out,
    };

    for field in parsed.fields() {
        // The thumbnail directory describes a picture we are not writing.
        if field.ifd_num != exif::In::PRIMARY {
            continue;
        }
        let ifd = match field.tag.0 {
            exif::Context::Tiff => Ifd::Primary,
            exif::Context::Exif => Ifd::Exif,
            exif::Context::Gps => Ifd::Gps,
            _ => continue,
        };
        let tag = field.tag.number();
        let skip = match ifd {
            Ifd::Primary => SKIP_PRIMARY,
            Ifd::Exif => SKIP_EXIF,
            Ifd::Gps => &[][..],
        };
        if skip.contains(&tag) {
            continue;
        }
        let value = match &field.value {
            exif::Value::Byte(v) => Value::Byte(v.clone()),
            exif::Value::Ascii(v) => {
                let text = v.first().map(|b| String::from_utf8_lossy(b).to_string())
                    .unwrap_or_default();
                let text = text.trim_end_matches('\0').trim().to_string();
                if text.is_empty() {
                    continue;
                }
                Value::Ascii(text)
            }
            exif::Value::Short(v) => Value::Short(v.clone()),
            exif::Value::Long(v) => Value::Long(v.clone()),
            exif::Value::Rational(v) => {
                Value::Rational(v.iter().map(|r| (r.num, r.denom)).collect())
            }
            exif::Value::SShort(v) => Value::SShort(v.clone()),
            exif::Value::SLong(v) => Value::SLong(v.clone()),
            exif::Value::SRational(v) => {
                Value::SRational(v.iter().map(|r| (r.num, r.denom)).collect())
            }
            exif::Value::Undefined(v, _) => Value::Undefined(v.clone()),
            // Float and Double are not EXIF types; anything else is unknown to
            // the reader and would be a guess to re-encode.
            _ => continue,
        };
        let size = match &value {
            Value::Byte(v) | Value::Undefined(v) => v.len(),
            Value::Ascii(s) => s.len(),
            Value::Short(v) => v.len() * 2,
            Value::SShort(v) => v.len() * 2,
            Value::Long(v) => v.len() * 4,
            Value::SLong(v) => v.len() * 4,
            Value::Rational(v) => v.len() * 8,
            Value::SRational(v) => v.len() * 8,
        };
        if size > MAX_VALUE {
            continue;
        }
        out.push(Entry::new(ifd, tag, value));
    }
    out
}

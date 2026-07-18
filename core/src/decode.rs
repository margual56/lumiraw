//! Raw decoding: file bytes -> linear sRGB, plus the EXIF the pipeline reasons about.

use crate::ops::{smoothstep, Image, Plane};

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

pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut cursor = std::io::Cursor::new(bytes);
    let raw = rawloader::decode(&mut cursor).map_err(|e| format!("{:?}", e))?;

    let mut meta = read_exif(bytes);
    if meta.make.is_empty() {
        meta.make = raw.clean_make.clone();
    }
    if meta.model.is_empty() {
        meta.model = raw.clean_model.clone();
    }

    let data: Vec<f32> = match &raw.data {
        rawloader::RawImageData::Integer(v) => v.iter().map(|x| *x as f32).collect(),
        rawloader::RawImageData::Float(v) => v.clone(),
    };
    if raw.cpp != 1 {
        // Already RGB (some DNGs / sRAW): just normalise.
        let (w, h) = (raw.width, raw.height);
        let mut img = Image::new(w, h);
        let black = raw.blacklevels[0] as f32;
        let white = (raw.whitelevels[0] as f32 - black).max(1.0);
        for i in 0..w * h * 3.min(raw.cpp * w * h) {
            img.d[i] = ((data[i] - black) / white).clamp(0.0, 1.0);
        }
        return Ok(Decoded { img: orient(&img, raw.orientation), meta });
    }

    // --- black/white levels + camera white balance ------------------------
    // dcraw normalises the multipliers by the largest one, so no channel can
    // clip further than the raw already did.
    let mut wb = [
        if raw.wb_coeffs[0].is_finite() && raw.wb_coeffs[0] > 0.0 { raw.wb_coeffs[0] } else { 1.0 },
        if raw.wb_coeffs[1].is_finite() && raw.wb_coeffs[1] > 0.0 { raw.wb_coeffs[1] } else { 1.0 },
        if raw.wb_coeffs[2].is_finite() && raw.wb_coeffs[2] > 0.0 { raw.wb_coeffs[2] } else { 1.0 },
    ];
    let wmax = wb[0].max(wb[1]).max(wb[2]);
    for g in wb.iter_mut() {
        *g /= wmax;
    }

    let (fw, fh) = (raw.width, raw.height);
    let mut cfa_plane = Plane::new(fw, fh);
    for y in 0..fh {
        for x in 0..fw {
            let c = raw.cfa.color_at(y, x);
            let black = raw.blacklevels[c.min(3)] as f32;
            let white = raw.whitelevels[c.min(3)] as f32;
            let v = (data[y * fw + x] - black) / (white - black).max(1.0);
            cfa_plane.d[y * fw + x] = v.clamp(0.0, 1.0) * wb[c.min(2)];
        }
    }

    // --- usable area ------------------------------------------------------
    let (ct, cr, cb, cl) = (raw.crops[0], raw.crops[1], raw.crops[2], raw.crops[3]);
    let (mut x0, mut y0) = (cl, ct);
    let mut w = fw.saturating_sub(cl + cr);
    let mut h = fh.saturating_sub(ct + cb);
    if w < 16 || h < 16 {
        x0 = 0;
        y0 = 0;
        w = fw;
        h = fh;
    }
    // Keep the CFA phase: an odd offset would swap red and blue.
    if x0 % 2 == 1 {
        x0 -= 1;
        w += 1;
    }
    if y0 % 2 == 1 {
        y0 -= 1;
        h += 1;
    }
    w = w.min(fw - x0);
    h = h.min(fh - y0);

    let mut cam = demosaic(&cfa_plane, &raw.cfa, x0, y0, w, h);
    reconstruct_highlights(&mut cam, &wb);

    // --- camera RGB -> linear sRGB ---------------------------------------
    let m = cam_to_srgb(&raw.xyz_to_cam);
    for px in cam.d.chunks_exact_mut(3) {
        let (r, g, b) = (px[0], px[1], px[2]);
        px[0] = (m[0][0] * r + m[0][1] * g + m[0][2] * b).max(0.0);
        px[1] = (m[1][0] * r + m[1][1] * g + m[1][2] * b).max(0.0);
        px[2] = (m[2][0] * r + m[2][1] * g + m[2][2] * b).max(0.0);
    }

    Ok(Decoded { img: orient(&cam, raw.orientation), meta })
}

/// Recover blown highlights to neutral instead of magenta.
fn reconstruct_highlights(img: &mut Image, wb: &[f32; 3]) {
    // Where recovery starts, as a fraction of each channel's own ceiling.
    const ONSET: f32 = 0.96;

    // Each channel's ceiling is where the white balance put its saturation
    // point, and these do not vary per pixel, so work them out once.
    let limit = [wb[0].max(1e-6), wb[1].max(1e-6), wb[2].max(1e-6)];
    let onset = [ONSET * limit[0], ONSET * limit[1], ONSET * limit[2]];

    for px in img.d.chunks_exact_mut(3) {
        let ceiling = px[0].max(px[1]).max(px[2]);
        for c in 0..3 {
            // `ceiling` is the largest of the three by construction, so the gap
            // is never negative and a channel already holding the ceiling is
            // left exactly where it is.
            let blown = smoothstep(onset[c], limit[c], px[c]);
            px[c] += blown * (ceiling - px[c]);
        }
    }
}

/// dcraw's `cam_xyz_coeff`: compose with the sRGB primaries, normalise each
/// row to unit sum (so neutral camera values stay neutral), then invert.
fn cam_to_srgb(xyz_to_cam: &[[f32; 3]; 4]) -> [[f32; 3]; 3] {
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
// demosaic, Malvar-He-Cutler, the 5x5 linear interpolator
// -------------------------------------------------------------------------

/// Gradient-corrected bilinear interpolation (Malvar, He & Cutler 2004).
fn demosaic(cfa: &Plane, pattern: &rawloader::CFA, x0: usize, y0: usize, w: usize, h: usize) -> Image {
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

fn orient(img: &Image, o: rawloader::Orientation) -> Image {
    use rawloader::Orientation::*;
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
    m
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The same thing through the real Bayer path.
    #[test]
    fn blown_bayer_region_develops_to_white() {
        let wb = [1.0f32, 0.37, 0.52];
        let cfa = rawloader::CFA::new("RGGB");
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
                    [-0.0819, 0.1706, 0.5785], [0.0, 0.0, 0.0]];
        let m = cam_to_srgb(&sony);
        let out = [m[0][0] + m[0][1] + m[0][2],
                   m[1][0] + m[1][1] + m[1][2],
                   m[2][0] + m[2][1] + m[2][2]];
        assert!(cast(out) < 1.01, "camera neutral drifted through the matrix: {out:?}");
    }
}

/// Everything in the original file worth carrying into the exported one.
pub fn collect_exif(bytes: &[u8]) -> Vec<crate::exif::Entry> {
    use crate::exif::{Entry, Ifd, Value};

    // Tags describing the raw's storage, or that we write ourselves.
    const SKIP_PRIMARY: &[u16] = &[
        0x0100, 0x0101, 0x0102, 0x0103, 0x0106, 0x0107, 0x0111, 0x0112, 0x0115, 0x0116,
        0x0117, 0x011C, 0x0118, 0x0119, 0x013D, 0x0142, 0x0143, 0x0144, 0x0145, 0x014A,
        0x0201, 0x0202, 0x828D, 0x828E, 0x8769, 0x8825, 0xC61A, 0xC61B, 0xC61C, 0xC61D,
        0xC61E, 0xC61F, 0xC620, 0xC621, 0xC622, 0xC623, 0xC624, 0xC625, 0xC626, 0xC627,
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

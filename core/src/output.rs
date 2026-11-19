//! Quantisation and delivery formats.

use crate::exif::{self, Entry, Ifd};
use crate::icc::{self, Intent, Profile};
use kit::Image;

/// Display-linear -> sRGB integers.
pub fn encode8(img: &Image, dither: bool) -> Vec<u8> {
    let mut out = vec![0u8; img.w * img.h * 3];
    let mut state: u32 = 0x9E3779B9;
    let mut rand = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 8) as f32 / 16_777_216.0
    };
    for i in 0..out.len() {
        let mut v = kit::srgb_encode_scalar(img.d[i]) * 255.0;
        if dither {
            // Triangular, one code value either way: the amplitude that takes
            // quantisation's error off the signal entirely.
            v += rand() - rand();
        }
        out[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

pub fn encode16(img: &Image) -> Vec<u16> {
    let mut out = vec![0u16; img.w * img.h * 3];
    for i in 0..out.len() {
        out[i] = (kit::srgb_encode_scalar(img.d[i]) * 65535.0).round().clamp(0.0, 65535.0) as u16;
    }
    out
}

/// RGBA8 for direct display in a canvas, what previews travel as.
pub fn rgba8(img: &Image) -> Vec<u8> {
    let mut out = vec![255u8; img.w * img.h * 4];
    for (dst, px) in out.chunks_exact_mut(4).zip(img.px()) {
        for c in 0..3 {
            dst[c] = (kit::srgb_encode_scalar(px[c]) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// RGBA8 sRGB, as a canvas holds it, back to display-linear.
pub fn from_rgba8(rgba: &[u8], w: usize, h: usize) -> Image {
    let mut out = Image::new(w, h);
    for (px, src) in out.px_mut().iter_mut().zip(rgba.chunks_exact(4)) {
        *px = [0, 1, 2].map(|c| kit::srgb_decode_scalar(src[c] as f32 / 255.0));
    }
    out
}

pub struct Format {
    pub id: &'static str,
    pub ext: &'static str,
    pub label: &'static str,
    pub mime: &'static str,
    /// Whether the format can hold CMYK. PNG and WebP are RGB only.
    pub cmyk: bool,
}

pub const FORMATS: [Format; 5] = [
    Format { id: "png8", ext: "png", label: "PNG (8-bit)", mime: "image/png", cmyk: false },
    Format { id: "png16", ext: "png", label: "PNG (16-bit)", mime: "image/png", cmyk: false },
    Format { id: "jpeg", ext: "jpg", label: "JPEG", mime: "image/jpeg", cmyk: true },
    Format { id: "webp", ext: "webp", label: "WebP", mime: "image/webp", cmyk: false },
    Format { id: "tiff", ext: "tif", label: "TIFF (8-bit)", mime: "image/tiff", cmyk: true },
];

pub fn format_spec(id: &str) -> &'static Format {
    FORMATS.iter().find(|f| f.id == id).unwrap_or(&FORMATS[0])
}

/// The colour space a file is written in.
#[derive(Clone, Copy, Debug)]
pub enum Space<'a> {
    /// What every screen and the web assume.
    Srgb,
    /// Ink, through a CMYK profile, for print.
    Cmyk(&'a Profile, Intent),
}

/// Encode the finished image in sRGB, carrying the original's metadata with it.
pub fn save(img: &Image, fmt: &str, quality: u8, meta: &[Entry], modified: Option<&str>)
    -> Result<Vec<u8>, String> {
    save_in(img, fmt, quality, meta, modified, Space::Srgb)
}

/// Encode the finished image in a colour space, with that space's profile
/// embedded where the format has room for one.
pub fn save_in(img: &Image, fmt: &str, quality: u8, meta: &[Entry], modified: Option<&str>,
               space: Space) -> Result<Vec<u8>, String> {
    check_space(fmt, space)?;
    let block = exif::build(meta, img.w as u32, img.h as u32, crate::SOFTWARE, modified);
    match fmt {
        "png16" => png_bytes(img, 16, &block, meta, modified),
        "jpeg" => jpeg_bytes(img, quality, &block, space),
        "tiff" => tiff_bytes(img, meta, modified, space),
        _ => png_bytes(img, 8, &block, meta, modified),
    }
}

/// Whether a file of this format can be written in this space: CMYK needs a
/// format that holds it and a CMYK profile.
pub fn check_space(fmt: &str, space: Space) -> Result<(), String> {
    let Space::Cmyk(profile, _) = space else { return Ok(()) };
    let spec = format_spec(fmt);
    if !spec.cmyk {
        return Err(format!("{} cannot hold CMYK; JPEG and TIFF can", spec.label));
    }
    profile.require_cmyk()
}

/// The picture's samples in the space, with the profile that describes them:
/// RGB or CMYK, 8 bits a channel.
fn samples<'a>(img: &Image, space: Space<'a>) -> Result<(Vec<u8>, &'a [u8]), String> {
    let rgb = encode8(img, true);
    match space {
        Space::Srgb => Ok((rgb, icc::srgb_bytes())),
        Space::Cmyk(profile, intent) => Ok((icc::to_cmyk8(&rgb, profile, intent)?, &profile.bytes)),
    }
}

/// The EXIF block on its own, for the formats encoded outside Rust.
pub fn exif_block(img: &Image, meta: &[Entry], modified: Option<&str>) -> Vec<u8> {
    exif::build(meta, img.w as u32, img.h as u32, crate::SOFTWARE, modified)
}

/// `YYYY:MM:DD HH:MM:SS` into its parts, for the formats that want numbers
/// rather than a string.
fn parts_of(stamp: &str) -> Option<(u16, u8, u8, u8, u8, u8)> {
    let bytes = stamp.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let n = |from: usize, to: usize| stamp.get(from..to)?.trim().parse::<u32>().ok();
    Some((n(0, 4)? as u16, n(5, 7)? as u8, n(8, 10)? as u8,
          n(11, 13)? as u8, n(14, 16)? as u8, n(17, 19)? as u8))
}

fn png_bytes(img: &Image, bits: u8, block: &[u8], meta: &[Entry], modified: Option<&str>)
    -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(if bits == 16 { png::BitDepth::Sixteen } else { png::BitDepth::Eight });
        enc.set_compression(png::Compression::Fast);
        enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        // PNG's own text chunks as well as the EXIF block: plenty of viewers
        // show these and never look at EXIF in a PNG.
        let _ = enc.add_text_chunk("Software".into(), crate::SOFTWARE.into());
        if let Some(when) = exif::captured_at(meta) {
            let _ = enc.add_text_chunk("Creation Time".into(), when);
        }
        for (keyword, tag, ifd) in [
            ("Source", 0x0110u16, Ifd::Primary),       // camera model
            ("Author", 0x013B, Ifd::Primary),          // artist
            ("Copyright", 0x8298, Ifd::Primary),
            ("Description", 0x010E, Ifd::Primary),
        ] {
            if let Some(text) = exif::text_of(meta, ifd, tag) {
                let _ = enc.add_text_chunk(keyword.into(), text);
            }
        }
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
        // eXIf has to precede the image data, which is where this lands.
        writer.write_chunk(png::chunk::ChunkType(*b"eXIf"), block).map_err(|e| e.to_string())?;
        // tIME is PNG's own last-modified field, and this file was made now.
        if let Some((y, mo, d, h, mi, sec)) = modified.and_then(parts_of) {
            let when = [(y >> 8) as u8, y as u8, mo, d, h, mi, sec];
            writer.write_chunk(png::chunk::ChunkType(*b"tIME"), &when)
                .map_err(|e| e.to_string())?;
        }
        if bits == 16 {
            let data = encode16(img);
            let mut bytes = Vec::with_capacity(data.len() * 2);
            for v in data {
                bytes.extend_from_slice(&v.to_be_bytes());
            }
            writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
        } else {
            writer.write_image_data(&encode8(img, true)).map_err(|e| e.to_string())?;
        }
    }
    Ok(buf)
}

fn jpeg_bytes(img: &Image, quality: u8, block: &[u8], space: Space)
    -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut buf, quality);
    // 4:4:4: no chroma subsampling.
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.set_progressive(true);
    // APP1, introduced by the "Exif\0\0" identifier the spec requires.
    let mut app1 = Vec::with_capacity(block.len() + 6);
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(block);
    enc.add_app_segment(1, &app1).map_err(|e| e.to_string())?;
    let (data, profile) = samples(img, space)?;
    enc.add_icc_profile(profile).map_err(|e| e.to_string())?;
    // The encoder writes CMYK the way Photoshop does (Adobe APP14, inverted),
    // which is what every reader expects of a CMYK JPEG.
    let colour = match space {
        Space::Srgb => jpeg_encoder::ColorType::Rgb,
        Space::Cmyk(..) => jpeg_encoder::ColorType::Cmyk,
    };
    enc.encode(&data, img.w as u16, img.h as u16, colour).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// TIFF is itself a directory format, so the descriptive tags go in at the top
/// level where any reader will find them.
fn tiff_bytes(img: &Image, meta: &[Entry], modified: Option<&str>, space: Space)
    -> Result<Vec<u8>, String> {
    use tiff::encoder::colortype::{CMYK8, RGB8};
    let (data, profile) = samples(img, space)?;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut buf).map_err(|e| e.to_string())?;
        // The encoder's colour type is a type parameter, so each space is its
        // own instantiation.
        let (w, h) = (img.w as u32, img.h as u32);
        match space {
            Space::Srgb => tiff_image::<_, RGB8>(&mut enc, w, h, &data, meta, modified, profile),
            Space::Cmyk(..) => tiff_image::<_, CMYK8>(&mut enc, w, h, &data, meta, modified, profile),
        }?;
    }
    Ok(buf.into_inner())
}

fn tiff_image<W: std::io::Write + std::io::Seek, C>(
    enc: &mut tiff::encoder::TiffEncoder<W>, w: u32, h: u32, data: &[u8], meta: &[Entry],
    modified: Option<&str>, profile: &[u8]) -> Result<(), String>
where C: tiff::encoder::colortype::ColorType<Inner = u8>, [u8]: tiff::encoder::TiffValue {
    let mut image = enc.new_image::<C>(w, h).map_err(|e| e.to_string())?;
    if C::TIFF_VALUE == tiff::tags::PhotometricInterpretation::CMYK {
        // InkSet: CMYK, in that order.
        let _ = image.encoder().write_tag(tiff::tags::Tag::Unknown(332), 1u16);
    }
    tiff_tags(image.encoder(), meta, modified, profile);
    image.write_data(data).map_err(|e| e.to_string())
}

fn tiff_tags<W: std::io::Write + std::io::Seek, K: tiff::encoder::TiffKind>(
    dir: &mut tiff::encoder::DirectoryEncoder<W, K>, meta: &[Entry], modified: Option<&str>,
    profile: &[u8]) {
    use tiff::tags::Tag;
    // The ICC profile, tag 34675, where every TIFF reader looks.
    let _ = dir.write_tag(Tag::Unknown(34675), profile);
    let _ = dir.write_tag(Tag::Software, crate::SOFTWARE);
    let _ = dir.write_tag(Tag::Orientation, 1u16);
    for (tag, number) in [
        (Tag::Make, 0x010Fu16),
        (Tag::Model, 0x0110),
        (Tag::Artist, 0x013B),
        (Tag::Copyright, 0x8298),
        (Tag::ImageDescription, 0x010E),
    ] {
        if let Some(text) = exif::text_of(meta, Ifd::Primary, number) {
            let _ = dir.write_tag(tag, text.as_str());
        }
    }
    // DateTime is when the file was written, which is now.
    if let Some(stamp) = modified {
        let _ = dir.write_tag(Tag::DateTime, &stamp[..stamp.len().min(19)]);
    } else if let Some(when) = exif::captured_at(meta) {
        let _ = dir.write_tag(Tag::DateTime, when.as_str());
    }
    if let Some(when) = exif::captured_at(meta) {
        let _ = dir.write_tag(Tag::Unknown(0x9003), when.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif::Value;

    fn frame() -> Image {
        let mut img = Image::new(8, 8);
        for (i, v) in img.d.iter_mut().enumerate() {
            *v = (i % 7) as f32 / 7.0;
        }
        img
    }

    /// What the camera recorded about the photograph, as it would arrive from
    /// `decode::collect_exif`.
    fn recorded() -> Vec<Entry> {
        vec![
            Entry::new(Ifd::Primary, 0x010F, Value::Ascii("SONY".into())),
            Entry::new(Ifd::Primary, 0x0110, Value::Ascii("ILCE-5000".into())),
            Entry::new(Ifd::Primary, 0x013B, Value::Ascii("Marcos".into())),
            Entry::new(Ifd::Exif, 0x829A, Value::Rational(vec![(1, 125)])),
            Entry::new(Ifd::Exif, 0x829D, Value::Rational(vec![(71, 10)])),
            Entry::new(Ifd::Exif, 0x8827, Value::Short(vec![100])),
            Entry::new(Ifd::Exif, 0x920A, Value::Rational(vec![(103, 1)])),
            Entry::new(Ifd::Exif, 0xA434, Value::Ascii("E 55-210mm F4.5-6.3 OSS".into())),
            Entry::new(Ifd::Exif, 0x9003, Value::Ascii("2026:09:22 15:04:05".into())),
            Entry::new(Ifd::Gps, 0x0001, Value::Ascii("N".into())),
            Entry::new(Ifd::Gps, 0x0002, Value::Rational(vec![(43, 1), (15, 1), (0, 1)])),
        ]
    }

    fn read_back(bytes: &[u8]) -> ::exif::Exif {
        ::exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(bytes))
            .expect("the exported file should carry readable metadata")
    }

    /// The whole point of the feature.
    #[test]
    fn exports_carry_the_cameras_metadata() {
        use ::exif::{In, Tag};
        for format in ["png8", "png16", "jpeg", "tiff"] {
            let bytes = save(&frame(), format, 92, &recorded(), None).expect("encode");
            let read = read_back(&bytes);
            let text = |tag: Tag| {
                read.get_field(tag, In::PRIMARY)
                    .map(|f| f.display_value().to_string().replace('"', ""))
            };
            assert_eq!(text(Tag::Make).as_deref(), Some("SONY"), "{format}: make lost");
            assert_eq!(text(Tag::Model).as_deref(), Some("ILCE-5000"), "{format}: model lost");
            assert_eq!(text(Tag::Artist).as_deref(), Some("Marcos"), "{format}: artist lost");
            assert_eq!(text(Tag::Software).as_deref(), Some(crate::SOFTWARE),
                       "{format}: not stamped");

            // TIFF gets the descriptive tags only: the encoder owns its own
            // directory and will not carry an Exif sub-IFD.
            if format == "tiff" {
                continue;
            }
            assert_eq!(text(Tag::ExposureTime).as_deref(), Some("1/125"), "{format}");
            assert_eq!(text(Tag::FNumber).as_deref(), Some("7.1"), "{format}");
            assert_eq!(text(Tag::PhotographicSensitivity).as_deref(), Some("100"), "{format}");
            assert_eq!(text(Tag::FocalLength).as_deref(), Some("103"), "{format}");
            assert_eq!(text(Tag::LensModel).as_deref(), Some("E 55-210mm F4.5-6.3 OSS"),
                       "{format}");
            assert_eq!(text(Tag::DateTimeOriginal).as_deref(), Some("2026-09-22 15:04:05"),
                       "{format}: capture time lost");
            assert!(read.get_field(Tag::GPSLatitude, In::PRIMARY).is_some(),
                    "{format}: where it was taken was lost");
            // Ours, describing the file we actually wrote.
            assert_eq!(text(Tag::PixelXDimension).as_deref(), Some("8"), "{format}");
            assert_eq!(text(Tag::Orientation).as_deref(),
                       Some("row 0 at top and column 0 at left"), "{format}");
        }
    }

    /// Two different times, and they must not be confused: the photograph was
    /// taken when the camera says, and the file was written when we say.
    #[test]
    fn capture_time_and_edit_time_stay_apart() {
        use ::exif::{In, Tag};
        let edited = "2026:09:22 16:30:00+02:00";
        for format in ["png8", "png16", "jpeg", "tiff"] {
            let bytes = save(&frame(), format, 92, &recorded(), Some(edited)).expect("encode");
            let read = read_back(&bytes);
            let text = |tag: Tag| {
                read.get_field(tag, In::PRIMARY)
                    .map(|f| f.display_value().to_string().replace('"', ""))
            };
            assert_eq!(text(Tag::DateTime).as_deref(), Some("2026-09-22 16:30:00"),
                       "{format}: the file's own time should be when it was written");
            if format == "tiff" {
                continue;
            }
            assert_eq!(text(Tag::DateTimeOriginal).as_deref(), Some("2026-09-22 15:04:05"),
                       "{format}: the capture time was overwritten");
            assert_eq!(text(Tag::OffsetTime).as_deref(), Some("+02:00"), "{format}");
        }
    }

    /// A raw that only recorded a plain DateTime still knows when it was taken,
    /// and that must survive being replaced by the edit stamp.
    #[test]
    fn a_plain_date_is_kept_as_the_capture_time() {
        use ::exif::{In, Tag};
        let only_a_file_date = vec![
            Entry::new(Ifd::Primary, 0x0132, Value::Ascii("2019:04:01 08:00:00".into())),
        ];
        let bytes = save(&frame(), "jpeg", 92, &only_a_file_date,
                         Some("2026:09:22 16:30:00")).expect("encode");
        let read = read_back(&bytes);
        let text = |tag: Tag| {
            read.get_field(tag, In::PRIMARY).map(|f| f.display_value().to_string())
        };
        assert_eq!(text(Tag::DateTimeOriginal).as_deref(), Some("2019-04-01 08:00:00"),
                   "the only record of when it was taken was lost");
        assert_eq!(text(Tag::DateTime).as_deref(), Some("2026-09-22 16:30:00"));
    }

    /// A frame with no metadata at all still has to produce a valid file.
    #[test]
    fn exports_without_metadata_still_decode() {
        for format in ["png8", "png16", "jpeg", "tiff"] {
            let bytes = save(&frame(), format, 92, &[], None).expect("encode");
            assert!(bytes.len() > 32, "{format}: produced nothing");
            let read = read_back(&bytes);
            use ::exif::{In, Tag};
            assert!(read.get_field(Tag::Software, In::PRIMARY).is_some(), "{format}");
        }
    }

    /// The JPEG's markers, as (marker, payload) up to the start of scan.
    fn markers(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let mut out = Vec::new();
        let mut i = 2;
        while i + 4 <= bytes.len() && bytes[i] == 0xFF {
            let marker = bytes[i + 1];
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            out.push((marker, bytes[i + 4..i + 2 + len].to_vec()));
            if marker == 0xDA {
                break;
            }
            i += 2 + len;
        }
        out
    }

    fn icc_in_jpeg(bytes: &[u8]) -> Vec<u8> {
        markers(bytes).into_iter()
            .filter(|(m, p)| *m == 0xE2 && p.starts_with(b"ICC_PROFILE\0"))
            .flat_map(|(_, p)| p[14..].to_vec())
            .collect()
    }

    /// A print shop gets four channels, the profile they are in, and still
    /// the camera's metadata.
    #[test]
    fn a_cmyk_jpeg_is_four_channels_with_its_profile() {
        let profile = Profile::builtin_cmyk();
        let bytes = save_in(&frame(), "jpeg", 92, &recorded(), None,
                            Space::Cmyk(&profile, Intent::Perceptual)).expect("encode");
        let all = markers(&bytes);
        let sof = all.iter().find(|(m, _)| *m == 0xC0 || *m == 0xC2).expect("a frame header");
        assert_eq!(sof.1[5], 4, "not four components");
        assert!(all.iter().any(|(m, p)| *m == 0xEE && p.starts_with(b"Adobe")),
                "no Adobe marker, so readers will not know how the CMYK is stored");
        assert_eq!(icc_in_jpeg(&bytes), profile.bytes, "the profile did not go with it");
        let read = read_back(&bytes);
        assert!(read.get_field(::exif::Tag::Model, ::exif::In::PRIMARY).is_some());
    }

    #[test]
    fn an_srgb_jpeg_says_so() {
        let bytes = save(&frame(), "jpeg", 92, &[], None).expect("encode");
        let icc = icc_in_jpeg(&bytes);
        assert!(!icc.is_empty(), "no profile embedded");
        assert_eq!(Profile::parse(&icc).unwrap().kind, icc::Kind::Rgb);
    }

    #[test]
    fn a_cmyk_tiff_is_separated_with_its_profile() {
        use tiff::decoder::{Decoder, DecodingResult};
        use tiff::tags::Tag;
        let profile = Profile::builtin_cmyk();
        let mut img = Image::new(4, 4);
        for v in img.d.iter_mut() {
            *v = 1.0;
        }
        let bytes = save_in(&img, "tiff", 92, &recorded(), None,
                            Space::Cmyk(&profile, Intent::Perceptual)).expect("encode");
        let mut dec = Decoder::new(std::io::Cursor::new(&bytes)).expect("a TIFF");
        assert_eq!(dec.colortype().unwrap(), tiff::ColorType::CMYK(8));
        // This decoder widens a BYTE array to u64s; the values are what count.
        let icc: Vec<u8> = dec.get_tag_u64_vec(Tag::Unknown(34675)).unwrap()
            .into_iter().map(|v| v as u8).collect();
        assert_eq!(icc, profile.bytes, "the profile did not go with it");
        match dec.read_image().unwrap() {
            // White paper takes no ink.
            DecodingResult::U8(px) => assert!(px.iter().all(|&v| v < 8), "{:?}", &px[..4]),
            _ => panic!("not 8-bit"),
        }
        let read = read_back(&bytes);
        assert!(read.get_field(::exif::Tag::Model, ::exif::In::PRIMARY).is_some());
    }

    #[test]
    fn formats_without_cmyk_refuse_it() {
        let profile = Profile::builtin_cmyk();
        for format in ["png8", "png16", "webp", "nonsense"] {
            assert!(save_in(&frame(), format, 92, &[], None,
                            Space::Cmyk(&profile, Intent::Perceptual)).is_err(),
                    "{format} took CMYK");
        }
    }
}

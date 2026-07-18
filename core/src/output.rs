//! Quantisation and delivery formats.

use crate::exif::{self, Entry, Ifd};
use crate::ops::{self, Image};

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
        let mut v = ops::srgb_encode_scalar(img.d[i]) * 255.0;
        if dither {
            v += (rand() - rand()) * 0.5;
        }
        out[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

pub fn encode16(img: &Image) -> Vec<u16> {
    let mut out = vec![0u16; img.w * img.h * 3];
    for i in 0..out.len() {
        out[i] = (ops::srgb_encode_scalar(img.d[i]) * 65535.0).round().clamp(0.0, 65535.0) as u16;
    }
    out
}

/// RGBA8 for direct display in a canvas, what previews travel as.
pub fn rgba8(img: &Image) -> Vec<u8> {
    let mut out = vec![255u8; img.w * img.h * 4];
    for i in 0..img.w * img.h {
        for c in 0..3 {
            out[i * 4 + c] =
                (ops::srgb_encode_scalar(img.d[i * 3 + c]) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

pub struct Format {
    pub id: &'static str,
    pub ext: &'static str,
    pub label: &'static str,
    pub mime: &'static str,
}

pub const FORMATS: [Format; 5] = [
    Format { id: "png8", ext: "png", label: "PNG (8-bit)", mime: "image/png" },
    Format { id: "png16", ext: "png", label: "PNG (16-bit)", mime: "image/png" },
    Format { id: "jpeg", ext: "jpg", label: "JPEG", mime: "image/jpeg" },
    Format { id: "webp", ext: "webp", label: "WebP", mime: "image/webp" },
    Format { id: "tiff", ext: "tif", label: "TIFF (8-bit)", mime: "image/tiff" },
];

pub fn format_spec(id: &str) -> &'static Format {
    FORMATS.iter().find(|f| f.id == id).unwrap_or(&FORMATS[0])
}

/// Encode the finished image, carrying the original's metadata with it.
pub fn save(img: &Image, fmt: &str, quality: u8, meta: &[Entry], modified: Option<&str>)
    -> Result<Vec<u8>, String> {
    let block = exif::build(meta, img.w as u32, img.h as u32, crate::SOFTWARE, modified);
    match fmt {
        "png16" => png_bytes(img, 16, &block, meta, modified),
        "jpeg" => jpeg_bytes(img, quality, &block),
        "tiff" => tiff_bytes(img, meta, modified),
        _ => png_bytes(img, 8, &block, meta, modified),
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

fn jpeg_bytes(img: &Image, quality: u8, block: &[u8]) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut buf, quality);
    // 4:4:4, like the Python version: no colour smearing.
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.set_progressive(true);
    // APP1, introduced by the "Exif\0\0" identifier the spec requires.
    let mut app1 = Vec::with_capacity(block.len() + 6);
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(block);
    enc.add_app_segment(1, &app1).map_err(|e| e.to_string())?;
    enc.encode(&encode8(img, true), img.w as u16, img.h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// TIFF is itself a directory format, so the descriptive tags go in at the top
/// level where any reader will find them.
fn tiff_bytes(img: &Image, meta: &[Entry], modified: Option<&str>)
    -> Result<Vec<u8>, String> {
    use tiff::tags::Tag;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut buf).map_err(|e| e.to_string())?;
        let mut image = enc
            .new_image::<tiff::encoder::colortype::RGB8>(img.w as u32, img.h as u32)
            .map_err(|e| e.to_string())?;
        {
            let dir = image.encoder();
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
        image.write_data(&encode8(img, true)).map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
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
}

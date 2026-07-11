//! Quantisation and delivery formats.

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

/// Encode the finished image.  `webp` is handled by the browser (the canvas
/// encoder), so it never reaches here.
pub fn save(img: &Image, fmt: &str, quality: u8) -> Result<Vec<u8>, String> {
    match fmt {
        "png16" => png_bytes(img, 16),
        "jpeg" => jpeg_bytes(img, quality),
        "tiff" => tiff_bytes(img),
        _ => png_bytes(img, 8),
    }
}

fn png_bytes(img: &Image, bits: u8) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(if bits == 16 { png::BitDepth::Sixteen } else { png::BitDepth::Eight });
        enc.set_compression(png::Compression::Fast);
        enc.set_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = enc.write_header().map_err(|e| e.to_string())?;
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

fn jpeg_bytes(img: &Image, quality: u8) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut buf, quality);
    // 4:4:4, like the Python version: no colour smearing.
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.set_progressive(true);
    enc.encode(&encode8(img, true), img.w as u16, img.h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn tiff_bytes(img: &Image) -> Result<Vec<u8>, String> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut buf).map_err(|e| e.to_string())?;
        enc.write_image::<tiff::encoder::colortype::RGB8>(
            img.w as u32,
            img.h as u32,
            &encode8(img, true),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

//! Colour management at the end of the pipeline: the space a file is written
//! in, and what a printer's profile will make of the picture (soft proofing).
//!
//! Everything before this works in display-linear sRGB, so every conversion
//! here starts from sRGB. A profile is either one the photographer loads (the
//! print shop's, usually) or the built-in Coated FOGRA39, the reference for
//! offset printing on coated paper in Europe.

use moxcms::{ColorProfile, DataColorSpace, Layout, ProfileText, RenderingIntent,
             Transform8BitExecutor, TransformOptions};
use std::sync::{Arc, OnceLock};

/// FOGRA39L, as colord ships it. Its copyright tag reads "This profile is free
/// of known copyright restrictions".
const FOGRA39: &[u8] = include_bytes!("icc/FOGRA39L_coated.icc");
const FOGRA39_NAME: &str = "Coated FOGRA39";

/// Big enough for any real output profile (the large CMYK ones are a few MB),
/// small enough that a wrong file is turned away before it is parsed.
const MAX_BYTES: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rgb,
    Cmyk,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::Rgb => "rgb",
            Kind::Cmyk => "cmyk",
        }
    }
}

/// How colours the profile cannot reach are brought into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    /// The whole picture is compressed to fit, keeping the relations between
    /// colours: the usual choice for photographs.
    Perceptual,
    /// Colours the profile can reach are kept exactly, the rest are clipped to
    /// its edge.
    Relative,
}

impl Intent {
    /// `perceptual` or `relative`, as the interface names them.
    pub fn from_id(id: &str) -> Option<Intent> {
        match id {
            "perceptual" => Some(Intent::Perceptual),
            "relative" => Some(Intent::Relative),
            _ => None,
        }
    }

    fn options(self) -> TransformOptions {
        TransformOptions {
            rendering_intent: match self {
                Intent::Perceptual => RenderingIntent::Perceptual,
                Intent::Relative => RenderingIntent::RelativeColorimetric,
            },
            ..Default::default()
        }
    }
}

/// An output profile, read and checked.
#[derive(Debug)]
pub struct Profile {
    pub name: String,
    pub kind: Kind,
    /// The file as given, for embedding in what is written with it.
    pub bytes: Vec<u8>,
    /// A digest of the file, so that the interface can tell one from another.
    pub id: String,
    pub builtin: bool,
    cms: ColorProfile,
}

impl Profile {
    /// Read an ICC profile. Only RGB and CMYK ones, the two kinds a
    /// photograph can be printed or shown through, are taken.
    pub fn parse(bytes: &[u8]) -> Result<Profile, String> {
        if bytes.len() > MAX_BYTES {
            return Err(format!("{} MB is far larger than any colour profile",
                               bytes.len() >> 20));
        }
        let cms = ColorProfile::new_from_slice(bytes)
            .map_err(|e| format!("that is not an ICC profile this can read ({e:?})"))?;
        let kind = match cms.color_space {
            DataColorSpace::Rgb => Kind::Rgb,
            DataColorSpace::Cmyk => Kind::Cmyk,
            other => return Err(format!(
                "that profile is for {other:?} data; this takes RGB and CMYK ones")),
        };
        let name = cms.description.as_ref().and_then(text).unwrap_or_default();
        let profile = Profile { name, kind, bytes: bytes.to_vec(), id: kit::digest(bytes),
                                builtin: false, cms };
        // A profile that parses but cannot be converted to (a device link, an
        // input-only profile) is no use here, and is better refused now than
        // at the end of an export.
        profile.from_srgb(Intent::Perceptual)?;
        Ok(profile)
    }

    /// Coated FOGRA39, for CMYK when nothing else has been loaded.
    pub fn builtin_cmyk() -> Profile {
        let mut p = Profile::parse(FOGRA39).expect("the built-in profile reads");
        p.name = FOGRA39_NAME.into();
        p.builtin = true;
        p
    }

    /// sRGB into this profile, 8 bits a channel: RGB, or CMYK with 255 for
    /// full ink.
    fn from_srgb(&self, intent: Intent) -> Result<Arc<Transform8BitExecutor>, String> {
        ColorProfile::new_srgb()
            .create_transform_8bit(Layout::Rgb, &self.cms, self.layout(), intent.options())
            .map_err(|e| format!("that profile cannot be converted to ({e:?})"))
    }

    /// CMYK can only be written through a CMYK profile.
    pub fn require_cmyk(&self) -> Result<(), String> {
        match self.kind {
            Kind::Cmyk => Ok(()),
            Kind::Rgb => Err(format!("{} is an RGB profile, and CMYK needs a CMYK one",
                                     self.name)),
        }
    }

    fn layout(&self) -> Layout {
        match self.kind {
            Kind::Rgb => Layout::Rgb,
            Kind::Cmyk => Layout::Rgba,
        }
    }
}

fn text(t: &ProfileText) -> Option<String> {
    let s = match t {
        ProfileText::PlainString(s) => s.clone(),
        ProfileText::Localizable(all) => {
            let english = all.iter().find(|l| l.language.eq_ignore_ascii_case("en"));
            english.or(all.first())?.value.clone()
        }
        ProfileText::Description(d) => {
            if d.unicode_string.trim().is_empty() { d.ascii_string.clone() }
            else { d.unicode_string.clone() }
        }
    };
    let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
    (!s.is_empty()).then_some(s)
}

/// The sRGB profile, encoded, for tagging files written in sRGB.
pub fn srgb_bytes() -> &'static [u8] {
    static SRGB: OnceLock<Vec<u8>> = OnceLock::new();
    SRGB.get_or_init(|| ColorProfile::new_srgb().encode().unwrap_or_default())
}

/// Points on a side of the proof's table: what proofing tools use, and close
/// to exact for the smooth round trip it holds.
const GRID: usize = 33;

/// A soft proof: sRGB to the profile and straight back, so the screen shows
/// what the profile does to the picture. The round trip is baked once into a
/// table, which is several times quicker than running both conversions on
/// every pixel of every preview.
pub struct Proof {
    /// sRGB in, sRGB out, 0..1, in `lut::trilinear`'s layout.
    table: Vec<f32>,
}

impl Proof {
    /// The way there uses the intent asked for, as the export will. The way
    /// back is relative colorimetric, so what the profile kept is shown as it
    /// is, with the paper as the screen's white.
    pub fn new(profile: &Profile, intent: Intent) -> Result<Proof, String> {
        let back = profile.cms
            .create_transform_16bit(profile.layout(), &ColorProfile::new_srgb(), Layout::Rgb,
                                    Intent::Relative.options())
            .map_err(|e| format!("that profile cannot be converted from ({e:?})"))?;
        let there = ColorProfile::new_srgb()
            .create_transform_16bit(Layout::Rgb, &profile.cms, profile.layout(), intent.options())
            .map_err(|e| format!("that profile cannot be converted to ({e:?})"))?;
        let level = |i: usize| ((i * 65535 + (GRID - 1) / 2) / (GRID - 1)) as u16;
        let mut grid = Vec::with_capacity(GRID * GRID * GRID * 3);
        for b in 0..GRID {
            for g in 0..GRID {
                for r in 0..GRID {
                    grid.extend_from_slice(&[level(r), level(g), level(b)]);
                }
            }
        }
        let mut device = vec![0u16; GRID * GRID * GRID * profile.layout().channels()];
        let mut out = vec![0u16; grid.len()];
        there.transform(&grid, &mut device).map_err(|e| format!("{e:?}"))?;
        back.transform(&device, &mut out).map_err(|e| format!("{e:?}"))?;
        let table = out.iter().map(|&v| v as f32 / 65535.0).collect();
        Ok(Proof { table })
    }

    /// Proof RGBA8 pixels in place. Alpha is left as it was.
    pub fn apply_rgba8(&self, rgba: &mut [u8]) {
        for px in rgba.chunks_exact_mut(4) {
            let at = [0, 1, 2].map(|c| px[c] as f32 / 255.0);
            let out = crate::lut::trilinear(&self.table, GRID, at);
            for c in 0..3 {
                // `as u8` saturates, and the table never leaves 0..1 anyway.
                px[c] = (out[c] * 255.0 + 0.5) as u8;
            }
        }
    }
}

/// A finished picture in a CMYK profile, 8 bits a channel, 255 for full ink.
pub fn to_cmyk8(srgb8: &[u8], profile: &Profile, intent: Intent) -> Result<Vec<u8>, String> {
    profile.require_cmyk()?;
    let to = profile.from_srgb(intent)?;
    let mut out = vec![0u8; srgb8.len() / 3 * 4];
    to.transform(srgb8, &mut out).map_err(|e| format!("{e:?}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kit::Image;

    /// The picture as the profile renders it, display-linear like everything
    /// else, for checking a proof without going through RGBA.
    fn proofed(img: &Image, proof: &Proof) -> Image {
        let mut rgba = crate::output::rgba8(img);
        proof.apply_rgba8(&mut rgba);
        crate::output::from_rgba8(&rgba, img.w, img.h)
    }

    fn patch(rgb: [f32; 3]) -> Image {
        let mut img = Image::new(2, 2);
        for px in img.px_mut() {
            *px = rgb.map(kit::srgb_decode_scalar);
        }
        img
    }

    fn encoded(img: &Image) -> [f32; 3] {
        img.px()[0].map(kit::srgb_encode_scalar)
    }

    #[test]
    fn the_built_in_profile_is_cmyk_and_named() {
        let p = Profile::builtin_cmyk();
        assert_eq!(p.kind, Kind::Cmyk);
        assert_eq!(p.name, FOGRA39_NAME);
        assert!(p.builtin);
    }

    #[test]
    fn not_a_profile_is_refused() {
        assert!(Profile::parse(b"LUT_3D_SIZE 2\n").is_err());
        assert!(Profile::parse(&[]).is_err());
    }

    #[test]
    fn an_encoded_srgb_profile_reads_back_as_rgb() {
        let p = Profile::parse(srgb_bytes()).expect("our own sRGB profile");
        assert_eq!(p.kind, Kind::Rgb);
        assert!(!p.id.is_empty());
    }

    /// Proofing through sRGB itself changes nothing worth seeing.
    #[test]
    fn proofing_through_srgb_is_the_identity() {
        let p = Profile::parse(srgb_bytes()).unwrap();
        let proof = Proof::new(&p, Intent::Relative).unwrap();
        for rgb in [[0.8, 0.3, 0.2], [0.1, 0.5, 0.9], [0.5, 0.5, 0.5]] {
            let got = encoded(&proofed(&patch(rgb), &proof));
            for c in 0..3 {
                assert!((got[c] - rgb[c]).abs() < 3.0 / 255.0, "{rgb:?} came back {got:?}");
            }
        }
    }

    /// Ink cannot reach the screen's purest blue, so a proof through a press
    /// profile has to show it duller; a grey stays grey.
    #[test]
    fn a_press_profile_dulls_what_ink_cannot_print() {
        let proof = Proof::new(&Profile::builtin_cmyk(), Intent::Relative).unwrap();
        let blue = encoded(&proofed(&patch([0.0, 0.0, 1.0]), &proof));
        assert!(blue[0] > 0.1 || blue[1] > 0.1 || blue[2] < 0.9,
                "pure blue survived a press profile untouched: {blue:?}");
        let grey = encoded(&proofed(&patch([0.5, 0.5, 0.5]), &proof));
        let spread = grey.iter().cloned().fold(f32::MIN, f32::max)
            - grey.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.04, "a mid grey came back tinted: {grey:?}");
    }

    #[test]
    fn white_is_no_ink_and_black_is_a_lot() {
        let p = Profile::builtin_cmyk();
        let cmyk = to_cmyk8(&[255, 255, 255, 0, 0, 0], &p, Intent::Perceptual).unwrap();
        assert!(cmyk[..4].iter().all(|&v| v < 8), "paper white was inked: {:?}", &cmyk[..4]);
        let total: u32 = cmyk[4..].iter().map(|&v| v as u32).sum();
        assert!(cmyk[7] > 200 && total > 600, "black was barely inked: {:?}", &cmyk[4..]);
        // The press limits total ink; FOGRA39 to about 330 %.
        assert!(total < 255 * 35 / 10, "more ink than the press takes: {:?}", &cmyk[4..]);
    }

    #[test]
    fn rgb_profiles_do_not_make_cmyk() {
        let p = Profile::parse(srgb_bytes()).unwrap();
        assert!(to_cmyk8(&[0, 0, 0], &p, Intent::Perceptual).is_err());
    }
}

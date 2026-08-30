//! LumiRaw's API: what can be done to a photograph, in a handful of calls.

mod bracket;
mod grades;
mod photo;
pub mod wasm;

pub use bracket::{finding_json, notes_json, Bracket, FrameInfo};
pub use grades::{clear_cube, curves, load_cube, looks, CubeInfo};
pub use photo::{Developed, Encoded, Export, Exported, Photo, Size};

pub use autoraw_core::grade::Settings as Edit;
pub use autoraw_core::merge::{Finding, Notes as MergeNotes, Options as MergeOptions};
pub use autoraw_core::raw::DecodeError;
pub use kit::{Image, Report};

/// A progress callback: the fraction done and a stage code the host names in
/// its own language. Every long operation takes one, and `None` is silence.
pub type Progress<'a> = Option<&'a mut dyn FnMut(f32, &str)>;

/// The build, as Cargo knows it, and as it signs every exported file.
pub const VERSION: &str = autoraw_core::VERSION;
pub const SOFTWARE: &str = autoraw_core::SOFTWARE;

/// The lens calibrations (a baked copy of lensfun's database), parsed once and
/// passed to every open, since vignetting, distortion and lateral CA are all
/// read from it.
pub struct Lenses(autoraw_core::lensdb::Database);

impl Lenses {
    pub fn parse(json: &[u8]) -> Result<Lenses, String> {
        autoraw_core::lensdb::Database::parse(json).map(Lenses)
    }
    /// How many lenses it knows.
    pub fn len(&self) -> usize {
        self.0.lenses.len()
    }
}

/// Open a raw file: decode it, read its metadata, find its lens.
pub fn open(name: &str, bytes: &[u8], lenses: Option<&Lenses>) -> Result<Photo, DecodeError> {
    if bytes.is_empty() {
        return Err(DecodeError::undecodable("empty file"));
    }
    autoraw_core::develop::Development::open(name, bytes, lenses.map(|l| &l.0)).map(Photo::new)
}

/// A small picture of a raw file, from the JPEG the camera embedded in it,
/// without developing anything. For filmstrips.
pub fn thumbnail(bytes: &[u8], long_edge: usize) -> Result<Image, DecodeError> {
    autoraw_core::decode::thumbnail(bytes, long_edge.max(16))
}

/// The export formats, for a picker: `{id, label, ext, mime}` each.
pub fn formats() -> serde_json::Value {
    serde_json::json!(autoraw_core::output::FORMATS.iter()
        .map(|f| serde_json::json!({"id": f.id, "label": f.label, "ext": f.ext, "mime": f.mime}))
        .collect::<Vec<_>>())
}

/// A description with the export formats added, which is how the interface
/// is told what it can offer for this photograph.
pub(crate) fn with_formats(description: serde_json::Value) -> serde_json::Value {
    kit::with(description, serde_json::json!({"formats": formats()}))
}

/// Pixels for a canvas: 8-bit sRGB with opaque alpha.
pub fn rgba8(img: &Image) -> Vec<u8> {
    autoraw_core::output::rgba8(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nothing_and_not_a_raw_are_refused_as_undecodable() {
        assert_eq!(open("x.arw", &[], None).err().unwrap().code, "undecodable");
        assert_eq!(open("x.arw", b"not a raw file", None).err().unwrap().code, "undecodable");
        assert!(thumbnail(b"not a raw file", 240).is_err());
    }

    #[test]
    fn an_export_starts_from_sensible_defaults() {
        let e = Export::new("jpeg");
        assert_eq!((e.format.as_str(), e.quality, e.long_edge, e.modified.as_deref()),
                   ("jpeg", 92, None, None));
        let ids: Vec<_> = formats().as_array().unwrap().iter()
            .map(|f| f["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["png8", "png16", "jpeg", "webp", "tiff"]);
    }

    #[test]
    fn looks_and_curves_come_without_a_photograph() {
        let looks = looks();
        assert_eq!(looks["looks"][0]["id"], "none");
        assert!(looks["looks"].as_array().unwrap().len() >= 2);
        assert_eq!(curves(&json!({}))["identity"], true);
        let bent = curves(&json!({"curves": {"look": "custom", "strength": 1,
            "rgb": [[0, 0], [0.5, 0.6], [1, 1]], "r": [], "g": [], "b": []}}));
        assert_eq!(bent["identity"], false);
        assert_eq!(bent["points"]["rgb"], 3);
    }

    /// A file that does not parse must leave the table that was loaded.
    #[test]
    fn a_bad_cube_keeps_the_good_one() {
        let cube = "LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let first = load_cube(cube.as_bytes()).unwrap();
        assert_eq!(first.size, 2);
        assert!(load_cube(b"nonsense").is_err());
        assert_eq!(autoraw_core::lut::describe().map(|d| d.0), Some(2));
        clear_cube();
        assert!(autoraw_core::lut::describe().is_none());
        assert_ne!(first.id, load_cube(format!("{cube}\n").as_bytes()).unwrap().id,
                   "a different file must give a different id");
        clear_cube();
    }

    #[test]
    fn an_empty_bracket_has_nothing_to_remove_or_merge() {
        let mut b = Bracket::new();
        assert!(b.is_empty() && !b.remove(0));
        assert!(b.merge(MergeOptions::default(), None, None).is_err());
        assert!(b.add("x.arw", b"not a raw").is_err());
    }
}

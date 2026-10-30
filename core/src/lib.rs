//! LumiRaw's pipeline, as one wasm module.
/// The one place the version is written down. Cargo owns it; the interface
/// reads it back through the wasm, and every exported file is stamped with it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How this program names itself in a file's metadata.
pub const SOFTWARE: &str = concat!("LumiRaw ", env!("CARGO_PKG_VERSION"));

pub mod analyze;
pub mod curve;
pub mod decode;
pub mod denoise;
pub mod develop;
pub mod effects;
pub mod exif;
pub mod geometry;
pub mod grade;
pub mod lensdb;
pub mod local;
pub mod looks;
pub mod lut;
pub mod merge;
pub mod masks;
pub mod mixer;
pub mod models;
pub mod output;
pub mod profile;
pub mod stack;
pub mod raw;
pub mod skin;
pub mod straighten;
pub mod wheels;

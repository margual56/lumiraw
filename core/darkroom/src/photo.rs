//! One photograph, open for developing.

use crate::{Edit, Image, Progress, Report};
use autoraw_core::develop::Development;
use autoraw_core::geometry::Rect;
use autoraw_core::output;
use serde_json::Value;

/// How big a development should be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    /// Scaled so its longer side is this many pixels (a preview).
    LongEdge(usize),
    /// Every pixel the sensor recorded (an export, or the 100 % view).
    Full,
}

impl Size {
    fn edge(self) -> Option<usize> {
        match self {
            Size::LongEdge(e) => Some(e),
            Size::Full => None,
        }
    }
}

/// A developed picture and everything the pipeline decided on the way.
pub struct Developed {
    pub image: Image,
    /// What each stage did, or why it did not.
    pub report: Report,
    /// The automatic corrections, each with what it decided and whether it can
    /// be switched off, for the interface's list.
    pub toggles: Value,
    /// The rectangle kept of the framing canvas, as fractions, when the edit
    /// crops or straightens.
    pub crop: Option<Rect>,
}

/// How to save a picture.
#[derive(Clone, Debug)]
pub struct Export {
    /// `png8`, `png16`, `jpeg`, `tiff` or `webp`.
    pub format: String,
    /// 1..100, for the lossy formats.
    pub quality: u8,
    /// Scale down so the longer side is this long; `None` keeps every pixel.
    pub long_edge: Option<usize>,
    /// The moment of the export (EXIF `YYYY:MM:DD HH:MM:SS+HH:MM`), for the
    /// file's own modified date. Passed in because wasm has no clock.
    pub modified: Option<String>,
}

impl Export {
    pub fn new(format: &str) -> Export {
        Export { format: format.to_string(), quality: 92, long_edge: None, modified: None }
    }
}

/// What an export encodes to.
pub enum Encoded {
    /// A finished file.
    File { bytes: Vec<u8>, mime: &'static str, ext: &'static str },
    /// WebP, which the host encodes (there is no pure-Rust encoder worth
    /// carrying).
    Canvas { exif: Vec<u8> },
}

pub struct Exported {
    pub image: Image,
    pub encoded: Encoded,
    /// When the photograph was taken, as the file records it, for dating the
    /// entry if the export goes into a zip.
    pub captured: Option<String>,
}

/// An open raw file. Decoding and lens correction are done once and kept, so
/// developing it again with different settings is cheap.
pub struct Photo {
    dev: Development,
}

impl Photo {
    pub(crate) fn new(dev: Development) -> Photo {
        Photo { dev }
    }

    /// Find this photograph's lens again in a set of calibrations that has
    /// grown since it was opened; see `Lenses::extend`.
    pub fn use_lenses(&mut self, lenses: &crate::Lenses) {
        self.dev.use_lenses(&lenses.0);
    }

    /// The lens mount whose calibrations `lenses` still lacks for this
    /// photograph, if any: the one to load before its lens can be corrected.
    pub fn missing_mount(&self, lenses: &crate::Lenses) -> Option<String> {
        self.dev.missing_mount(&lenses.0)
    }

    pub fn width(&self) -> usize {
        self.dev.width()
    }
    pub fn height(&self) -> usize {
        self.dev.height()
    }

    /// Size, camera, lens, capture profile and notes, and the export formats,
    /// as the interface shows them.
    pub fn describe(&self) -> Value {
        crate::with_formats(self.dev.describe())
    }

    /// Develop with these settings, cropped to the framing.
    pub fn develop(&mut self, edit: &Edit, size: Size, progress: Progress) -> Developed {
        self.render(edit, size, true, progress)
    }

    /// Develop with these settings on the whole framing canvas, uncropped:
    /// what the framing step shows while a crop is being placed.
    pub fn develop_canvas(&mut self, edit: &Edit, size: Size, progress: Progress) -> Developed {
        self.render(edit, size, false, progress)
    }

    fn render(&mut self, edit: &Edit, size: Size, crop: bool, progress: Progress) -> Developed {
        let (image, report) = self.dev.render(edit, size.edge(), crop, progress);
        let toggles = self.dev.toggles(edit, &report);
        let crop = autoraw_core::geometry::effective_crop(self.dev.width(), self.dev.height(),
                                                          &edit.framing);
        Developed { image, report, toggles, crop }
    }

    /// The same photograph with every automatic correction switched off (the
    /// photographer's own choices kept), for a before/after comparison.
    pub fn before(&mut self, edit: &Edit, size: Size) -> Image {
        self.dev.baseline(edit, size.edge())
    }

    /// Develop at export size and encode.
    pub fn export(&mut self, edit: &Edit, export: &Export, mut progress: Progress)
                  -> Result<Exported, String> {
        let modified = export.modified.as_deref().filter(|s| !s.is_empty());
        let image = {
            let mut scaled = progress.as_mut().map(|cb| {
                let f: Box<dyn FnMut(f32, &str) + '_> = Box::new(move |f, code| cb(f * 0.8, code));
                f
            });
            let inner: Progress = match scaled.as_mut() {
                Some(b) => Some(&mut **b),
                None => None,
            };
            self.dev.render(edit, export.long_edge, true, inner).0
        };
        let meta = &self.dev.exif;
        // When the photograph was taken, so that a bundle can date each member
        // by it rather than by the moment the zip was written.
        let captured = meta.iter()
            .find(|e| e.tag == autoraw_core::exif::TAG_DATE_TIME_ORIGINAL)
            .or_else(|| meta.iter().find(|e| e.tag == autoraw_core::exif::TAG_DATE_TIME))
            .and_then(|e| match &e.value {
                autoraw_core::exif::Value::Ascii(s) => Some(s.trim_end_matches('\0').to_string()),
                _ => None,
            });
        if let Some(cb) = progress.as_mut() {
            cb(0.85, "encoding");
        }
        let encoded = if export.format == "webp" {
            Encoded::Canvas { exif: output::exif_block(&image, meta, modified) }
        } else {
            let bytes = output::save(&image, &export.format, export.quality.clamp(1, 100), meta,
                                     modified)?;
            let spec = output::format_spec(&export.format);
            Encoded::File { bytes, mime: spec.mime, ext: spec.ext }
        };
        if let Some(cb) = progress.as_mut() {
            cb(1.0, "done");
        }
        Ok(Exported { image, encoded, captured })
    }

    /// The camera's metadata as read from the file, as the export carries it.
    pub fn exif(&self) -> &[autoraw_core::exif::Entry] {
        &self.dev.exif
    }

    pub(crate) fn from_merge(name: &str, image: Image, meta: autoraw_core::decode::Meta,
                             exif: Vec<autoraw_core::exif::Entry>,
                             lenses: Option<&crate::Lenses>) -> Photo {
        Photo::new(Development::from_frame(name, image, meta, exif, lenses.map(|l| &l.0)))
    }
}

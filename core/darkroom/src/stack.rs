//! Many frames of one scene stacked into one photograph: averaged, the median,
//! the brightest, or the sharpest of each part.

use crate::{DecodeError, Lenses, Photo};
use lumiraw_core::merge::Frame;
use lumiraw_core::stack::{Added, Mode, Notes, Options, Stacker};
use serde_json::{json, Value};

pub use lumiraw_core::stack::Mode as StackMode;

/// A stack being gathered: each frame is decoded, lined up and folded in as
/// it arrives, and let go.
pub struct Stack(Stacker);

impl Stack {
    pub fn new(mode: Mode, align: bool) -> Stack {
        Stack(Stacker::new(Options::new(mode, align)))
    }

    /// Decode a frame and fold it in.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> Result<Added, DecodeError> {
        Ok(self.0.add(Frame::open(name, bytes)?).clone())
    }

    /// The stacked photograph, ready to develop. The page reports the
    /// progress, frame by frame, so this does not.
    pub fn finish(self, lenses: Option<&Lenses>) -> Result<(Photo, Value), String> {
        let mode = self.0.mode();
        let (image, reference, notes) = self.0.finish()?;
        let photo = Photo::from_merge(&format!("{} (stacked)", reference.name), image,
                                      reference.meta, reference.exif, lenses);
        Ok((photo, notes_json(mode, &notes)))
    }
}

/// One frame's fate, as the interface reads it.
pub fn added_json(a: &Added) -> Value {
    json!({
        "index": a.index,
        "left_out": a.left_out,
        "shift": [a.shift.0, a.shift.1],
        "scale": (a.scale * 10000.0).round() / 10000.0,
        "stops": (a.stops * 100.0).round() / 100.0,
    })
}

fn notes_json(mode: Mode, notes: &Notes) -> Value {
    json!({
        "mode": mode.code(),
        "frames": notes.added.len(),
        "used": notes.used,
        "added": notes.added.iter().map(added_json).collect::<Vec<_>>(),
        "findings": notes.findings.iter().map(crate::finding_json).collect::<Vec<_>>(),
    })
}

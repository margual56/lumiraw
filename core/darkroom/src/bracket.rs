//! Several exposures of one scene, merged into one photograph with more range.

use crate::{DecodeError, Finding, Lenses, MergeNotes, MergeOptions, Photo, Progress};
use lumiraw_core::merge::{self, Frame};
use serde_json::{json, Value};

/// What was learned about a frame on adding it, for the interface's list.
pub struct FrameInfo {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub iso: f32,
    pub shutter_s: f32,
    pub aperture: f32,
    /// What the exposure dial was set to, which is not always what the camera
    /// managed to do.
    pub exposure_comp: f32,
    /// How much light it was given, in the merge's own units.
    pub exposure: f32,
}

/// The frames of a bracket, gathered one at a time.
#[derive(Default)]
pub struct Bracket {
    frames: Vec<Frame>,
}

impl Bracket {
    pub fn new() -> Bracket {
        Bracket::default()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Decode a frame and add it.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> Result<FrameInfo, DecodeError> {
        let frame = Frame::open(name, bytes)?;
        let info = FrameInfo {
            name: name.to_string(),
            width: frame.linear.w,
            height: frame.linear.h,
            iso: frame.meta.iso,
            shutter_s: frame.meta.shutter_s,
            aperture: frame.meta.aperture,
            exposure_comp: frame.meta.exposure_comp,
            exposure: frame.exposure(),
        };
        self.frames.push(frame);
        Ok(info)
    }

    /// Drop a frame, without decoding the others again. False if there is no
    /// such frame.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.frames.len() {
            return false;
        }
        self.frames.remove(index);
        true
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    /// What is odd about the bracket so far, from the metadata alone, so that a
    /// bracket the camera could not deliver is on screen before the minute of
    /// merging rather than after it.
    pub fn check(&self) -> Vec<Finding> {
        merge::inspect(&self.frames)
    }

    /// Merge the frames into one photograph, ready to develop.
    pub fn merge(&mut self, options: MergeOptions, lenses: Option<&Lenses>, mut progress: Progress)
                 -> Result<(Photo, MergeNotes), String> {
        if let Some(cb) = progress.as_mut() {
            cb(0.15, "merging");
        }
        let (image, notes) = merge::merge(&self.frames, options)?;
        if let Some(cb) = progress.as_mut() {
            cb(0.8, "describing");
        }
        let reference = &self.frames[notes.reference];
        let photo = Photo::from_merge(&format!("{} (merged)", reference.name), image,
                                      reference.meta.clone(), reference.exif.clone(), lenses);
        self.frames.clear();
        if let Some(cb) = progress.as_mut() {
            cb(1.0, "done");
        }
        Ok((photo, notes))
    }
}

/// A finding as the interface reads it: a code and its numbers, never a
/// sentence, since the wording belongs to the locale files.
pub fn finding_json(f: &Finding) -> Value {
    json!({"code": f.code, "frames": f.frames, "value": (f.value * 100.0).round() / 100.0})
}

/// What the merge decided, as the interface shows it.
pub fn notes_json(notes: &MergeNotes) -> Value {
    let ev = |v: f32| (v * 100.0).round() / 100.0;
    json!({
        "frames": notes.stops.len(),
        "reference": notes.reference,
        "stops": notes.stops.iter().map(|v| ev(*v)).collect::<Vec<_>>(),
        "range_stops": ev(notes.range_stops),
        "shifts": notes.shifts.iter().map(|(x, y)| vec![*x, *y]).collect::<Vec<_>>(),
        "ghosted": (notes.ghosted * 10000.0).round() / 10000.0,
        // What the deghosting was set to, so the interface can say so when
        // it was the merge rather than the photographer that chose it.
        "deghost_used": (notes.deghost_used * 100.0).round() / 100.0,
        "uncovered": (notes.uncovered * 10000.0).round() / 10000.0,
        // What the photographer asked for, against what the frames turned
        // out to be. These differ when the camera ran out of shutter.
        "bias": notes.bias.iter().map(|v| ev(*v)).collect::<Vec<_>>(),
        "intended_range_stops": ev(notes.intended_range_stops),
        "findings": notes.findings.iter().map(finding_json).collect::<Vec<_>>(),
        // Where the pixels put each frame, and whether that had to be used
        // in place of metadata that could not be right.
        "measured": notes.measured.iter().map(|v| v.map(ev)).collect::<Vec<_>>(),
        "remeasured": notes.remeasured,
    })
}

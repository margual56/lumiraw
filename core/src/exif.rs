//! Carrying the camera's own metadata into the exported file.

/// A TIFF value. Only the types EXIF actually uses are represented; anything
/// else in the source file is dropped rather than guessed at.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(Vec<u8>),
    Ascii(String),
    Short(Vec<u16>),
    Long(Vec<u32>),
    Rational(Vec<(u32, u32)>),
    Undefined(Vec<u8>),
    SShort(Vec<i16>),
    SLong(Vec<i32>),
    SRational(Vec<(i32, i32)>),
}

/// Which directory a tag belongs to. The primary IFD describes the picture as
/// a file, the Exif IFD how it was taken, the GPS IFD where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ifd {
    Primary,
    Exif,
    Gps,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub tag: u16,
    pub ifd: Ifd,
    pub value: Value,
}

impl Entry {
    pub fn new(ifd: Ifd, tag: u16, value: Value) -> Entry {
        Entry { tag, ifd, value }
    }
}

// Tags this module writes itself, whatever the original said.
pub const TAG_ORIENTATION: u16 = 0x0112;
pub const TAG_SOFTWARE: u16 = 0x0131;
pub const TAG_EXIF_POINTER: u16 = 0x8769;
pub const TAG_GPS_POINTER: u16 = 0x8825;
pub const TAG_COLOR_SPACE: u16 = 0xA001;
pub const TAG_PIXEL_X: u16 = 0xA002;
pub const TAG_PIXEL_Y: u16 = 0xA003;
pub const TAG_DATE_TIME: u16 = 0x0132;
pub const TAG_DATE_TIME_ORIGINAL: u16 = 0x9003;

fn type_code(v: &Value) -> u16 {
    match v {
        Value::Byte(_) => 1,
        Value::Ascii(_) => 2,
        Value::Short(_) => 3,
        Value::Long(_) => 4,
        Value::Rational(_) => 5,
        Value::Undefined(_) => 7,
        Value::SShort(_) => 8,
        Value::SLong(_) => 9,
        Value::SRational(_) => 10,
    }
}

fn count(v: &Value) -> u32 {
    match v {
        Value::Byte(x) => x.len() as u32,
        // ASCII values are NUL terminated and the terminator counts.
        Value::Ascii(s) => s.len() as u32 + 1,
        Value::Short(x) => x.len() as u32,
        Value::Long(x) => x.len() as u32,
        Value::Rational(x) => x.len() as u32,
        Value::Undefined(x) => x.len() as u32,
        Value::SShort(x) => x.len() as u32,
        Value::SLong(x) => x.len() as u32,
        Value::SRational(x) => x.len() as u32,
    }
}

fn payload(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    match v {
        Value::Byte(x) | Value::Undefined(x) => out.extend_from_slice(x),
        Value::Ascii(s) => {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        Value::Short(x) => x.iter().for_each(|n| out.extend(n.to_le_bytes())),
        Value::Long(x) => x.iter().for_each(|n| out.extend(n.to_le_bytes())),
        Value::SShort(x) => x.iter().for_each(|n| out.extend(n.to_le_bytes())),
        Value::SLong(x) => x.iter().for_each(|n| out.extend(n.to_le_bytes())),
        Value::Rational(x) => x.iter().for_each(|(n, d)| {
            out.extend(n.to_le_bytes());
            out.extend(d.to_le_bytes());
        }),
        Value::SRational(x) => x.iter().for_each(|(n, d)| {
            out.extend(n.to_le_bytes());
            out.extend(d.to_le_bytes());
        }),
    }
    out
}

/// Size of a directory on disk: the entry count, the entries, and the offset
/// of the next directory.
fn ifd_size(n: usize) -> u32 {
    2 + 12 * n as u32 + 4
}

fn take(entries: &[Entry], ifd: Ifd, skip: &[u16]) -> Vec<Entry> {
    let mut out: Vec<Entry> = entries
        .iter()
        .filter(|e| e.ifd == ifd && !skip.contains(&e.tag))
        .cloned()
        .collect();
    // A TIFF directory has to be in ascending tag order, and readers do rely
    // on it: one that binary searches will simply miss anything out of place.
    out.sort_by_key(|e| e.tag);
    out.dedup_by_key(|e| e.tag);
    out
}

/// Build the EXIF block for an exported image.
pub fn build(entries: &[Entry], width: u32, height: u32, software: &str) -> Vec<u8> {
    let mut primary = take(entries, Ifd::Primary,
                           &[TAG_ORIENTATION, TAG_SOFTWARE, TAG_EXIF_POINTER, TAG_GPS_POINTER]);
    let mut exif = take(entries, Ifd::Exif, &[TAG_PIXEL_X, TAG_PIXEL_Y, TAG_COLOR_SPACE]);
    let gps = take(entries, Ifd::Gps, &[]);

    // The frame was rotated upright during decoding, so a viewer must not
    // rotate it again.
    primary.push(Entry::new(Ifd::Primary, TAG_ORIENTATION, Value::Short(vec![1])));
    primary.push(Entry::new(Ifd::Primary, TAG_SOFTWARE, Value::Ascii(software.to_string())));
    // 1 is sRGB, which is what everything downstream of the pipeline assumes.
    exif.push(Entry::new(Ifd::Exif, TAG_COLOR_SPACE, Value::Short(vec![1])));
    exif.push(Entry::new(Ifd::Exif, TAG_PIXEL_X, Value::Long(vec![width])));
    exif.push(Entry::new(Ifd::Exif, TAG_PIXEL_Y, Value::Long(vec![height])));
    primary.sort_by_key(|e| e.tag);
    exif.sort_by_key(|e| e.tag);

    // Offsets have to be known before anything is written, which is why the
    // directories are counted first.
    let n_primary = primary.len() + 1 + usize::from(!gps.is_empty());
    let off_exif = 8 + ifd_size(n_primary);
    let off_gps = off_exif + ifd_size(exif.len());
    let data_start = off_gps + if gps.is_empty() { 0 } else { ifd_size(gps.len()) };

    primary.push(Entry::new(Ifd::Primary, TAG_EXIF_POINTER, Value::Long(vec![off_exif])));
    if !gps.is_empty() {
        primary.push(Entry::new(Ifd::Primary, TAG_GPS_POINTER, Value::Long(vec![off_gps])));
    }
    primary.sort_by_key(|e| e.tag);

    let mut out: Vec<u8> = Vec::new();
    out.extend(b"II");                       // little endian
    out.extend(42u16.to_le_bytes());         // the answer, as the spec has it
    out.extend(8u32.to_le_bytes());          // the primary directory follows

    let mut data: Vec<u8> = Vec::new();
    write_ifd(&mut out, &primary, &mut data, data_start);
    write_ifd(&mut out, &exif, &mut data, data_start);
    if !gps.is_empty() {
        write_ifd(&mut out, &gps, &mut data, data_start);
    }
    out.extend(data);
    out
}

fn write_ifd(out: &mut Vec<u8>, entries: &[Entry], data: &mut Vec<u8>, data_start: u32) {
    out.extend((entries.len() as u16).to_le_bytes());
    for e in entries {
        out.extend(e.tag.to_le_bytes());
        out.extend(type_code(&e.value).to_le_bytes());
        out.extend(count(&e.value).to_le_bytes());
        let bytes = payload(&e.value);
        if bytes.len() <= 4 {
            // Short values live in the entry itself rather than out in the
            // data area, padded to the full four bytes.
            let mut inline = [0u8; 4];
            inline[..bytes.len()].copy_from_slice(&bytes);
            out.extend(inline);
        } else {
            out.extend((data_start + data.len() as u32).to_le_bytes());
            data.extend(bytes);
            if data.len() % 2 == 1 {
                data.push(0); // values start on a word boundary
            }
        }
    }
    out.extend(0u32.to_le_bytes()); // no directory after this one
}

/// The value of a tag, if the original file carried it, as text.
pub fn text_of(entries: &[Entry], ifd: Ifd, tag: u16) -> Option<String> {
    entries.iter().find(|e| e.ifd == ifd && e.tag == tag).and_then(|e| match &e.value {
        Value::Ascii(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}

/// When the photograph was taken, preferring the original capture time over
/// any later modification date.
pub fn captured_at(entries: &[Entry]) -> Option<String> {
    text_of(entries, Ifd::Exif, TAG_DATE_TIME_ORIGINAL)
        .or_else(|| text_of(entries, Ifd::Primary, TAG_DATE_TIME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(block: &[u8]) -> exif::Exif {
        exif::Reader::new().read_raw(block.to_vec()).expect("the block should parse")
    }

    /// The block we write has to be readable by something that did not write
    /// it, which is the only test that really matters here.
    #[test]
    fn round_trips_through_a_reader() {
        let entries = vec![
            Entry::new(Ifd::Primary, 0x010F, Value::Ascii("SONY".into())),   // Make
            Entry::new(Ifd::Primary, 0x0110, Value::Ascii("ILCE-5000".into())), // Model
            Entry::new(Ifd::Exif, 0x829A, Value::Rational(vec![(1, 125)])),  // ExposureTime
            Entry::new(Ifd::Exif, 0x829D, Value::Rational(vec![(71, 10)])),  // FNumber
            Entry::new(Ifd::Exif, 0x8827, Value::Short(vec![100])),          // ISO
            Entry::new(Ifd::Exif, TAG_DATE_TIME_ORIGINAL,
                       Value::Ascii("2026:09:22 15:04:05".into())),
            Entry::new(Ifd::Gps, 0x0002, Value::Rational(vec![(40, 1), (25, 1), (0, 1)])),
        ];
        let block = build(&entries, 4000, 3000, "autoraw 0.3.0");
        let read = parse(&block);

        use exif::{In, Tag};
        let text = |tag: Tag| {
            read.get_field(tag, In::PRIMARY).map(|f| f.display_value().to_string())
        };
        assert_eq!(text(Tag::Make).as_deref(), Some("\"SONY\""));
        assert_eq!(text(Tag::Model).as_deref(), Some("\"ILCE-5000\""));
        assert_eq!(text(Tag::ExposureTime).as_deref(), Some("1/125"));
        assert_eq!(text(Tag::PhotographicSensitivity).as_deref(), Some("100"));
        assert_eq!(text(Tag::DateTimeOriginal).as_deref(), Some("2026-09-22 15:04:05"));
        assert_eq!(text(Tag::Software).as_deref(), Some("\"autoraw 0.3.0\""));

        // Ours, not the original's: the export is upright and this size.
        assert_eq!(text(Tag::Orientation).as_deref(), Some("row 0 at top and column 0 at left"));
        assert_eq!(text(Tag::PixelXDimension).as_deref(), Some("4000"));
        assert_eq!(text(Tag::PixelYDimension).as_deref(), Some("3000"));
        // and the GPS directory survived as its own IFD
        assert!(read.get_field(Tag::GPSLatitude, In::PRIMARY).is_some(),
                "the GPS directory was lost");
    }

    /// Tags the original recorded about the raw's own storage, or about an
    /// orientation we have already applied, must not be copied blindly.
    #[test]
    fn overrides_beat_the_original() {
        let entries = vec![
            Entry::new(Ifd::Primary, TAG_ORIENTATION, Value::Short(vec![8])), // rotated 270
            Entry::new(Ifd::Primary, TAG_SOFTWARE, Value::Ascii("the camera".into())),
            Entry::new(Ifd::Exif, TAG_PIXEL_X, Value::Long(vec![5470])),
        ];
        let read = parse(&build(&entries, 800, 600, "autoraw 0.3.0"));
        use exif::{In, Tag};
        let value = |tag: Tag| read.get_field(tag, In::PRIMARY).unwrap().display_value().to_string();
        assert_eq!(value(Tag::Orientation), "row 0 at top and column 0 at left");
        assert_eq!(value(Tag::Software), "\"autoraw 0.3.0\"");
        assert_eq!(value(Tag::PixelXDimension), "800");
    }

    /// A file with no GPS must not grow an empty GPS directory, and one with
    /// nothing at all must still produce a block a reader accepts.
    #[test]
    fn copes_with_nothing_to_copy() {
        let block = build(&[], 10, 10, "autoraw");
        let read = parse(&block);
        use exif::{In, Tag};
        assert!(read.get_field(Tag::GPSLatitude, In::PRIMARY).is_none());
        assert_eq!(read.get_field(Tag::PixelXDimension, In::PRIMARY).unwrap()
                       .display_value().to_string(), "10");
    }
}

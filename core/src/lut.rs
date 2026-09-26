//! A 3D lookup table, read from a `.cube` file.

use kit::Image;
use std::cell::{Cell, RefCell};

/// The largest cube this will read.
pub const MAX_SIZE: usize = 64;

/// The smallest that is still a table rather than a typo.
pub const MIN_SIZE: usize = 2;

#[derive(Clone, Debug)]
pub struct Lut {
    pub size: usize,
    pub title: String,
    min: [f32; 3],
    max: [f32; 3],
    /// `size^3` triples, red changing fastest, as the format specifies.
    data: Vec<f32>,
}

thread_local! {
    /// The one loaded table, if any.
    static CURRENT: RefCell<Option<Lut>> = const { RefCell::new(None) };
    /// Bumped whenever the table in force changes, so a cached render can tell
    /// that the same settings would now come out differently.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Hold this table as the one in force, or clear it.
pub fn set(lut: Option<Lut>) {
    CURRENT.with(|c| *c.borrow_mut() = lut);
    GENERATION.with(|g| g.set(g.get() + 1));
}

/// Which table is in force, as a number that changes whenever it does.
pub fn generation() -> u64 {
    GENERATION.with(|g| g.get())
}

/// Whether a table is loaded, and what it is.
pub fn describe() -> Option<(usize, String)> {
    CURRENT.with(|c| c.borrow().as_ref().map(|l| (l.size, l.title.clone())))
}

/// Apply the table in force, if there is one and any of it is wanted.
pub fn apply_current(img: &mut Image, strength: f32) -> bool {
    let s = strength.clamp(0.0, 1.0);
    if s <= 0.0 {
        return false;
    }
    CURRENT.with(|c| match c.borrow().as_ref() {
        None => false,
        Some(lut) => {
            // In place.
            lut.apply_to(img, s);
            true
        }
    })
}

impl Lut {
    /// Parse a `.cube` file.
    pub fn parse(text: &str) -> Result<Lut, String> {
        let mut size = 0usize;
        let mut title = String::new();
        let mut min = [0.0f32; 3];
        let mut max = [1.0f32; 3];
        let mut data: Vec<f32> = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let head = parts.next().unwrap_or("");
            match head.to_ascii_uppercase().as_str() {
                "TITLE" => {
                    title = line[head.len()..].trim().trim_matches('"').to_string();
                }
                "LUT_1D_SIZE" => {
                    // A 1D cube is three curves, which this program already has
                    // a better editor for.
                    return Err("that is a 1D cube. Use the curve editor for those, or \
                                load a 3D one.".into());
                }
                "LUT_3D_SIZE" => {
                    size = parts.next().and_then(|v| v.parse().ok())
                        .ok_or("LUT_3D_SIZE is not a number")?;
                    if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
                        return Err(format!(
                            "the file says it is {size} on a side. This reads {MIN_SIZE} to \
                             {MAX_SIZE}, and 33 is what most tools write."));
                    }
                    data.reserve(size * size * size * 3);
                }
                "DOMAIN_MIN" | "DOMAIN_MAX" => {
                    let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if v.len() < 3 {
                        return Err(format!("{head} needs three numbers"));
                    }
                    let into = if head.eq_ignore_ascii_case("DOMAIN_MIN") { &mut min } else { &mut max };
                    into.copy_from_slice(&v[..3]);
                }
                _ => {
                    // Anything else has to be a triple, and a triple before
                    // the size is known is a file whose header is missing.
                    let Ok(first) = head.parse::<f32>() else { continue };
                    if size == 0 {
                        return Err("the file has colour values before it says how big it is, \
                                    so it is not a cube this can read.".into());
                    }
                    let rest: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if rest.len() < 2 {
                        return Err("a line of colour values is not three numbers".into());
                    }
                    data.push(first);
                    data.push(rest[0]);
                    data.push(rest[1]);
                }
            }
        }

        if size == 0 {
            return Err("no LUT_3D_SIZE in the file, so it is not a cube.".into());
        }
        let want = size * size * size * 3;
        if data.len() != want {
            return Err(format!(
                "the file says it is {size} on a side, which needs {} colours, but it has {}.",
                want / 3, data.len() / 3));
        }
        for i in 0..3 {
            // Written as a comparison against `Greater` rather than as `<=` so
            // that a NaN in the header is rejected too.
            if max[i].partial_cmp(&min[i]) != Some(std::cmp::Ordering::Greater) {
                return Err("the domain in the file is empty, backwards or not a number.".into());
            }
        }
        Ok(Lut { size, title, min, max, data })
    }

    fn sample(&self, i: usize, j: usize, k: usize, c: usize) -> f32 {
        // Red changes fastest, then green, then blue: the format's order.
        self.data[((k * self.size + j) * self.size + i) * 3 + c]
    }

    /// Apply to a display-linear image, blended by `strength`.
    #[must_use = "the graded picture is returned, not written into the one passed in"]
    pub fn apply(&self, img: &Image, strength: f32) -> Image {
        let mut out = img.clone();
        self.apply_to(&mut out, strength);
        out
    }

    fn apply_to(&self, img: &mut Image, strength: f32) {
        let n = self.size - 1;
        for px in img.px_mut() {
            let src = *px;
            let enc = src.map(kit::srgb_encode_scalar);
            // Into the table's own domain, which is 0..1 unless the file says
            // otherwise.
            let mut base = [0usize; 3];
            let mut frac = [0f32; 3];
            for c in 0..3 {
                let t = ((enc[c] - self.min[c]) / (self.max[c] - self.min[c])).clamp(0.0, 1.0)
                    * n as f32;
                let i = (t as usize).min(n.saturating_sub(1));
                base[c] = i;
                frac[c] = t - i as f32;
            }
            let (i, j, k) = (base[0], base[1], base[2]);
            let (fr, fg, fb) = (frac[0], frac[1], frac[2]);
            let step = if n == 0 { 0 } else { 1 };
            for c in 0..3 {
                // Trilinear: eight corners, three lerps.
                let c000 = self.sample(i, j, k, c);
                let c100 = self.sample(i + step, j, k, c);
                let c010 = self.sample(i, j + step, k, c);
                let c110 = self.sample(i + step, j + step, k, c);
                let c001 = self.sample(i, j, k + step, c);
                let c101 = self.sample(i + step, j, k + step, c);
                let c011 = self.sample(i, j + step, k + step, c);
                let c111 = self.sample(i + step, j + step, k + step, c);
                let c00 = c000 + (c100 - c000) * fr;
                let c10 = c010 + (c110 - c010) * fr;
                let c01 = c001 + (c101 - c001) * fr;
                let c11 = c011 + (c111 - c011) * fr;
                let c0 = c00 + (c10 - c00) * fg;
                let c1 = c01 + (c11 - c01) * fg;
                let graded = kit::srgb_decode_scalar((c0 + (c1 - c0) * fb).clamp(0.0, 1.0));
                px[c] = src[c] * (1.0 - strength) + graded * strength;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest cube that is still a cube, written out by hand: two on a
    /// side is eight corners, and the identity among them.
    fn identity_cube() -> String {
        let mut s = String::from("TITLE \"flat\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    s.push_str(&format!("{r}.0 {g}.0 {b}.0\n"));
                }
            }
        }
        s
    }

    fn grey(v: f32) -> Image {
        let mut img = Image::new(1, 1);
        for c in 0..3 {
            img.d[c] = v;
        }
        img
    }

    /// A table that maps every colour to itself has to come back with the
    /// picture intact, or the interpolation is wrong in a way no other test
    /// will localise.
    #[test]
    fn an_identity_cube_changes_nothing() {
        let lut = Lut::parse(&identity_cube()).expect("parse");
        assert_eq!(lut.size, 2);
        assert_eq!(lut.title, "flat");
        for level in [0.02f32, 0.18, 0.5, 0.9] {
            let out = lut.apply(&grey(level), 1.0);
            for c in 0..3 {
                assert!((out.d[c] - level).abs() < 2e-3,
                        "{level} came back {} in channel {c}", out.d[c]);
            }
        }
    }

    /// And one that does something has to do it by the amount asked, so the
    /// strength slider is a fade rather than a switch.
    #[test]
    fn strength_fades_the_table() {
        // Every corner to black: the whole picture should go with it.
        let mut text = String::from("LUT_3D_SIZE 2\n");
        for _ in 0..8 {
            text.push_str("0.0 0.0 0.0\n");
        }
        let lut = Lut::parse(&text).expect("parse");
        let full = lut.apply(&grey(0.5), 1.0);
        assert!(full.d[0] < 1e-6, "the table did not reach the picture: {}", full.d[0]);
        let half = lut.apply(&grey(0.5), 0.5);
        assert!((half.d[0] - 0.25).abs() < 1e-6, "half strength gave {}", half.d[0]);
        let none = lut.apply(&grey(0.5), 0.0);
        assert!((none.d[0] - 0.5).abs() < 1e-9, "zero strength changed the picture");
    }

    /// A cube is read on encoded values.
    #[test]
    fn a_cube_is_read_on_encoded_values() {
        // Three on a side, identity except the centre entry, which goes dark.
        let mut text = String::from("LUT_3D_SIZE 3\n");
        for b in 0..3 {
            for g in 0..3 {
                for r in 0..3 {
                    let mid = r == 1 && g == 1 && b == 1;
                    let v = |i: usize| if mid { 0.2 } else { i as f32 / 2.0 };
                    text.push_str(&format!("{} {} {}\n", v(r), v(g), v(b)));
                }
            }
        }
        let lut = Lut::parse(&text).expect("parse");
        let out = lut.apply(&grey(kit::srgb_decode_scalar(0.5)), 1.0);
        let want = kit::srgb_decode_scalar(0.2);
        assert!((out.d[0] - want).abs() < 1e-3,
                "mid grey came back {} rather than {want}", out.d[0]);
    }

    /// Every way a file can be wrong has to come back as a sentence about that
    /// file, because the person reading it chose the file and can choose
    /// another.
    #[test]
    fn a_bad_file_says_what_is_wrong_with_it() {
        let cases = [
            ("", "not a cube"),
            ("LUT_1D_SIZE 16\n0.0 0.0 0.0\n", "1D cube"),
            ("LUT_3D_SIZE 128\n", "128 on a side"),
            ("LUT_3D_SIZE 1\n", "1 on a side"),
            ("0.5 0.5 0.5\nLUT_3D_SIZE 2\n", "before it says how big"),
            ("LUT_3D_SIZE 2\n0.0 0.0 0.0\n", "but it has 1"),
        ];
        for (text, wanted) in cases {
            let err = Lut::parse(text).expect_err(&format!("{text:?} parsed"));
            assert!(err.contains(wanted), "{text:?} said {err:?}, wanted {wanted:?}");
        }
    }

    /// Comments, blank lines and a declared domain are all ordinary in files
    /// written by real tools, and none of them may throw the reader off.
    #[test]
    fn the_ordinary_decorations_are_read() {
        let mut text = String::from("# made by something\n\nTITLE \"with a domain\"\n\
                                     DOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n\n");
        text.push_str(&identity_cube().replace("TITLE \"flat\"\n", ""));
        let lut = Lut::parse(&text).expect("parse");
        assert_eq!(lut.title, "with a domain");
        let out = lut.apply(&grey(0.4), 1.0);
        assert!((out.d[0] - 0.4).abs() < 2e-3, "the domain lines moved the picture");
    }

    /// The table in force has to change the picture it is handed, through the
    /// entry point the pipeline actually calls.
    #[test]
    fn the_table_in_force_changes_the_picture() {
        let flat = Lut::parse(&format!("LUT_3D_SIZE 2\n{}", "0.5 0.5 0.5\n".repeat(8))).unwrap();
        set(Some(flat));
        let mut img = grey(0.9);
        assert!(apply_current(&mut img, 1.0), "a loaded table was not applied");
        set(None);
        let want = kit::srgb_decode_scalar(0.5);
        assert!((img.d[0] - want).abs() < 1e-3,
                "the picture came back {} rather than {want}: the table changed nothing", img.d[0]);
        let mut untouched = grey(0.9);
        assert!(!apply_current(&mut untouched, 1.0), "with no table there is nothing to apply");
        assert_eq!(untouched.d[0], 0.9);
    }
}

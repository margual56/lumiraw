//! The printer's profile, kept for the session the way the 3D table is:
//! loaded once, used by every export and proof after it.
//!
//! The printer's profile is the one loaded, or Coated FOGRA39 when none is.
//! CMYK exports are written through it, and a preview asked for with a proof
//! is shown as it will come out of it. Exports are never proofed.

use lumiraw_core::icc::{Intent, Profile, Proof};
use serde_json::{json, Value};
use std::cell::RefCell;

thread_local! {
    /// The printer's profile; `None` until first needed, then the built-in.
    static PRINTER: RefCell<Option<Profile>> = const { RefCell::new(None) };
    /// The last proof baked, with the profile and intent it was baked for.
    static PROOF: RefCell<Option<(String, Intent, Proof)>> = const { RefCell::new(None) };
}

/// Run `f` with the printer's profile.
pub fn with_profile<T>(f: impl FnOnce(&Profile) -> T) -> T {
    PRINTER.with(|p| f(p.borrow_mut().get_or_insert_with(Profile::builtin_cmyk)))
}

/// The printer's profile, as the interface shows it.
pub fn describe() -> Value {
    with_profile(|p| json!({
        "name": p.name, "kind": p.kind.id(), "id": p.id, "builtin": p.builtin,
        "bytes": p.bytes.len(),
    }))
}

/// Read a profile and make it the printer's. A file that cannot be read
/// leaves the printer's profile as it was.
pub fn load_profile(bytes: &[u8]) -> Result<Value, String> {
    let profile = Profile::parse(bytes)?;
    PRINTER.with(|p| *p.borrow_mut() = Some(profile));
    Ok(describe())
}

/// Back to the built-in profile.
pub fn clear_profile() -> Value {
    PRINTER.with(|p| *p.borrow_mut() = None);
    describe()
}

/// Proof RGBA8 preview pixels in place through the printer's profile. The
/// proof is baked once per profile and intent.
pub fn proof_rgba8(rgba: &mut [u8], intent: Intent) -> Result<(), String> {
    with_profile(|profile| PROOF.with(|cell| {
        let mut cell = cell.borrow_mut();
        let fresh = matches!(&*cell, Some((id, i, _)) if *id == profile.id && *i == intent);
        if !fresh {
            *cell = Some((profile.id.clone(), intent, Proof::new(profile, intent)?));
        }
        if let Some((_, _, proof)) = cell.as_ref() {
            proof.apply_rgba8(rgba);
        }
        Ok(())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb() -> &'static [u8] {
        lumiraw_core::icc::srgb_bytes()
    }

    fn cmyk_ready() -> Result<(), String> {
        with_profile(Profile::require_cmyk)
    }

    #[test]
    fn the_printer_is_fogra39_until_told_otherwise() {
        clear_profile();
        let d = describe();
        assert_eq!(d["kind"], "cmyk");
        assert_eq!(d["builtin"], true);
        assert!(cmyk_ready().is_ok());
    }

    #[test]
    fn an_rgb_printer_cannot_take_cmyk_and_forgetting_it_can() {
        load_profile(srgb()).unwrap();
        assert_eq!(describe()["kind"], "rgb");
        assert!(cmyk_ready().is_err());
        clear_profile();
        assert!(cmyk_ready().is_ok());
    }

    #[test]
    fn a_proof_follows_the_printers_profile() {
        clear_profile();
        let mut blue = [0u8, 0, 255, 255];
        proof_rgba8(&mut blue, Intent::Relative).unwrap();
        assert_ne!(blue, [0, 0, 255, 255], "the press proof left pure blue alone");

        // Through sRGB itself, the proof is close to nothing.
        load_profile(srgb()).unwrap();
        let mut blue = [0u8, 0, 255, 255];
        proof_rgba8(&mut blue, Intent::Relative).unwrap();
        assert!(blue[2] > 250 && blue[0] < 5, "{blue:?}");
        clear_profile();
    }

    #[test]
    fn a_bad_file_keeps_the_good_profile() {
        load_profile(srgb()).unwrap();
        assert!(load_profile(b"not a profile").is_err());
        assert_eq!(describe()["kind"], "rgb");
        clear_profile();
    }
}

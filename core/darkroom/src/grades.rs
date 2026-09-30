//! The grade's building blocks that exist without a photograph: the named
//! looks, the curve tables a settings object comes to, and 3D tables.

use crate::Edit;
use serde_json::{json, Value};

/// Every named look, for the list. The tables are not here: the page fetches
/// one when it is first chosen and hands it in with `load_look`.
pub fn looks() -> Value {
    json!({"looks": lumiraw_core::looks::LOOKS.iter().map(|l| json!({
        "id": l.id, "label": l.label, "description": l.description,
    })).collect::<Vec<_>>()})
}

/// Hold a named look's table, packed as `tools/looks.py` packs it. Refused
/// for a name that is not a look, so nothing can be parked under one.
pub fn load_look(id: &str, bytes: &[u8]) -> Result<(), String> {
    let look = lumiraw_core::looks::find(id).filter(|l| l.id != "none")
        .ok_or_else(|| format!("there is no look called {id}"))?;
    let lut = lumiraw_core::lut::Lut::from_bytes(look.label, bytes)?;
    lumiraw_core::lut::set_named(look.id, lut);
    Ok(())
}

/// The curves a settings object comes to, baked into tables as the pipeline
/// will evaluate them, so a plot drawn from these cannot drift from the
/// picture.
pub fn curves(raw: &Value) -> Value {
    let edit = Edit::from_json(raw);
    let stack = &edit.curves;
    let table = |c: &lumiraw_core::curve::Curve| {
        c.table().iter().map(|v| kit::round_to(*v, 4)).collect::<Vec<_>>()
    };
    let counts = stack.counts();
    let tabs = lumiraw_core::curve::edited(raw.get("curves"));
    json!({
        "identity": stack.is_identity(),
        "size": lumiraw_core::curve::TABLE,
        "rgb": table(stack.composite()),
        "r": table(stack.channel(0)),
        "g": table(stack.channel(1)),
        "b": table(stack.channel(2)),
        "edited": {"regions": table(&tabs[0]), "rgb": table(&tabs[1]), "r": table(&tabs[2]),
                   "g": table(&tabs[3]), "b": table(&tabs[4])},
        "points": {"rgb": counts[0], "r": counts[1], "g": counts[2], "b": counts[3]},
    })
}

/// A 3D table, as loaded.
pub struct CubeInfo {
    pub size: usize,
    pub title: String,
    /// A digest of the file, for a cache key: loading a different file has to
    /// give a different key, or a second cube is served the first one's render.
    pub id: String,
}

/// Read a `.cube` file and make it the table every development applies (at the
/// settings' `lut` strength).
pub fn load_cube(bytes: &[u8]) -> Result<CubeInfo, String> {
    let lut = lumiraw_core::lut::Lut::parse(&String::from_utf8_lossy(bytes))?;
    let (size, title) = (lut.size, lut.title.clone());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    lumiraw_core::lut::set(Some(lut));
    Ok(CubeInfo { size, title, id: format!("{h:x}") })
}

/// Forget the 3D table.
pub fn clear_cube() {
    lumiraw_core::lut::set(None);
}

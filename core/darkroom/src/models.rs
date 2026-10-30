//! The networks the developer uses, held once each was fetched: the face
//! detector, for reading skin. The page fetches each the first time something
//! needs it, as it does the looks' tables.

use critic::graph::Graph;
use std::cell::RefCell;
use std::collections::HashMap;

/// The names a network may be held under, and nothing else.
pub const NAMES: [&str; 1] = ["face-detector"];

thread_local! {
    static MODELS: RefCell<HashMap<&'static str, Graph>> = RefCell::new(HashMap::new());
}

/// Hold a network, baked as `tools/bake-faces.mjs` bakes them.
pub fn load(name: &str, bytes: &[u8]) -> Result<(), String> {
    let name = NAMES.iter().find(|n| **n == name).ok_or_else(|| format!("no network is called {name}"))?;
    let graph = Graph::parse(bytes)?;
    MODELS.with(|m| m.borrow_mut().insert(name, graph));
    Ok(())
}

/// Run `f` with a network, or say it has not been loaded.
pub fn with<T>(name: &str, f: impl FnOnce(&Graph) -> T) -> Result<T, String> {
    MODELS.with(|m| m.borrow().get(name).map(f)).ok_or_else(|| format!("{name} is not loaded"))
}

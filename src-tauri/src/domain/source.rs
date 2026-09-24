//! Where a resource comes from: the mod that provides it, or nothing for the game's own.

use std::collections::HashMap;

/// The resources a mod provides, by resource GUID. Everything the game itself provides is absent, so a
/// GUID missing here reads as the base game.
pub type ModSources = HashMap<String, String>;

/// The mod that provides a resource, or `None` when it comes from the game
pub fn source_of(sources: &ModSources, id: &str) -> Option<String> {
    sources.get(id).cloned()
}
//! Where a resource comes from: the mod that provides it, or nothing for the game's own.

use std::collections::HashMap;

/// The resources a mod provides, by resource GUID; a GUID absent here comes from the game itself
pub type ModSources = HashMap<String, String>;

/// The mod that provides a resource, or `None` for the game's own
pub fn source_of(sources: &ModSources, id: &str) -> Option<String> {
    sources.get(id).cloned()
}

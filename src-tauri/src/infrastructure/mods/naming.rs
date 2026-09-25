//! Naming a mod and decoding the text its banks carry: the label the UI shows its resources under.

use maclarian::formats::lsx::LsxNode;

use crate::infrastructure::archives::Archives;

use super::read::{attr, read_bank};

/// The name a mod is labeled with: the `Name` its own `meta.lsx` declares, which is what the game and
/// the mod manager call it. The archive's file name is a hash-suffixed stem (`hairunlocked_e4aaf48d-…`),
/// so it is only the fallback — for a mod that ships no `meta.lsx`, or one whose file cannot be read.
pub(super) fn mod_display_name(
    archives: &mut Archives,
    index: usize,
    paths: &[String],
    fallback: String,
) -> String {
    let Some(path) = paths.iter().find(|path| path.ends_with("meta.lsx")) else {
        return fallback;
    };

    read_bank(archives, index, path)
        .and_then(|document| {
            document
                .regions
                .iter()
                .flat_map(|region| region.nodes.iter())
                .find_map(module_name)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
}

/// The `Name` attribute of the first `ModuleInfo` node in the tree. The declaration sits one level
/// down (`Root > Config > ModuleInfo`), but the depth is not part of the format, so the walk simply
/// descends until it finds the node.
fn module_name(node: &LsxNode) -> Option<String> {
    if node.id == "ModuleInfo" {
        let name = attr(node, "Name");
        if !name.is_empty() {
            return Some(decode_entities(&name));
        }
    }

    node.children.iter().find_map(module_name)
}

/// XML entities decoded in a value read out of an attribute: the LSX reader hands the text over as it
/// stands in the file, and mod names do use escapes (`PixellBytes&apos; Adjustable Party Limit`),
/// which would otherwise reach the UI as markup. `&amp;` goes last so an escaped survives.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }

    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

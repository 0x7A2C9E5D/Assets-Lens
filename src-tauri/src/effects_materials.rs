//! Material names from the effects material banks.
//!
//! The database build only parses the `_merged.lsf` files maclarian keeps for character assets
//! (`Public/…/Content/Assets/**/[PAK]_Armor|Clothing|Body|…`, see `is_relevant_asset_path`), so a
//! material a visual references can be missing from it: VFX materials live under
//! `Content/Assets/Effects/Materials/[PAK]_*` and that whole folder is filtered out. The visual's
//! `MaterialID` is still valid, so the panel would render a bare GUID with no name and no template
//! to show.
//!
//! Reading those banks fills exactly that gap. The headers are read for every material of every
//! bank at once — a bank holds hundreds of materials and the archive read, not the parse, is the
//! cost — so the caller can keep the whole map and never read these files again.
//!
//! Only the header is taken: `ID`, `Name` and `SourceFile`. The textures a VFX material binds stay
//! unparsed, so a material read here keeps the empty binding list it already had and the material
//! section simply shows no chips for it.

use std::collections::HashMap;
use std::path::Path;

use maclarian::formats::common::{extract_value, TypeId};
use maclarian::formats::lsf::{parse_lsf_bytes, LsfDocument};

use crate::archives::Archives;

/// Archives shipping the effects material banks. These are the two maclarian's own build reads
/// (`Shared.pak` plus `GustavX.pak`), and the banks live in both: `Public/Shared/…` in the first,
/// `Public/GustavX/…` in the second.
const BANK_PAKS: &[&str] = &["Shared.pak", "GustavX.pak"];

/// Folder holding those banks. Paths come out of `Archives` normalized (`/` separators, lowercase)
/// and are matched with a substring test, so this is spelled the same way.
const BANK_FOLDER: &str = "content/assets/effects/materials/";

/// A material's name and the template it is derived from, as the material row shows them.
pub struct MaterialHeader {
    /// Human-readable name from `MaterialBank` (e.g. `VFX_Model_…_Fire_Beam_001`)
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
}

/// Read the header of every material the effects material banks define, keyed by material GUID.
///
/// A bank that cannot be listed, read or parsed contributes nothing: these names only decorate a
/// material row, and a missing archive must not take a detail view down with it.
pub fn read_headers(pool: &mut Archives, game_path: &Path) -> HashMap<String, MaterialHeader> {
    let mut headers = HashMap::new();

    for pak in BANK_PAKS {
        let pak_path = game_path.join(pak);
        for path in pool.list_matching(&pak_path, BANK_FOLDER) {
            // The folder also holds the odd non-merged file; only the merged ones are banks
            if !path.ends_with("_merged.lsf") {
                continue;
            }
            let Ok(bytes) = pool.read_in(&pak_path, &path) else {
                continue;
            };
            let Ok(doc) = parse_lsf_bytes(&bytes) else {
                continue;
            };
            headers.extend(headers_of(&doc));
        }
    }

    headers
}

/// Every material header of one document: the `Resource` nodes sitting directly under a
/// `MaterialBank` region, in document order.
///
/// The bank is found through the flat node list rather than by asking each bank for its children
/// (`find_children_by_name` walks every node per call): one bank holds hundreds of materials while
/// the document holds tens of thousands of nodes, and `Resource` is a name `TextureBank` uses too.
fn headers_of(doc: &LsfDocument) -> Vec<(String, MaterialHeader)> {
    let mut headers = Vec::new();

    for (index, node) in doc.nodes.iter().enumerate() {
        if doc.node_name(index) != Some("Resource") {
            continue;
        }
        if node.parent_index < 0
            || doc.node_name(node.parent_index as usize) != Some("MaterialBank")
        {
            continue;
        }

        let attrs = doc.attributes_of(index);
        let Some(id) = text_of(doc, &attrs, "ID").filter(|id| !id.is_empty()) else {
            continue;
        };
        headers.push((
            id,
            MaterialHeader {
                name: text_of(doc, &attrs, "Name").unwrap_or_default(),
                source_file: text_of(doc, &attrs, "SourceFile").unwrap_or_default(),
            },
        ));
    }

    headers
}

/// One attribute of `node` read as text, whatever type it is stored in.
///
/// `ID` is a `FixedString`, while `Name` and `SourceFile` are `LSString`s — and
/// `LsfDocument::get_fixed_string_attr` only accepts `FixedString`, silently returning `None` for
/// the other two, which is why this goes through the raw values instead.
fn text_of(
    doc: &LsfDocument,
    attrs: &[(usize, &str, TypeId, usize, usize)],
    want: &str,
) -> Option<String> {
    let (_, _, type_id, offset, length) = attrs.iter().find(|(_, name, ..)| *name == want)?;
    extract_value(&doc.values, *offset, *length, *type_id).ok()
}

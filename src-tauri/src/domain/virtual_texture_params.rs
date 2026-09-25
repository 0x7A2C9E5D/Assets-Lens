//! Virtual texture parameter names, read straight out of the LSF data.
//!
//! maclarian parses a `VirtualTextureParameters` node down to its `ID` — `MaterialDef` carries a list
//! of GUIDs — so the parameter a binding fills (`virtualtexture`, `overlayvirtualtexture`, …) never
//! reaches the app and has to be read from the documents instead.
//!
//! It is read from the material's *template*, not from the document the material itself lives in: a
//! merged document is always named `_merged.lsf`, so the file holding a given material cannot be
//! derived from that material. `MaterialDef::source_file`, by contrast, names a real template file
//! whose `VirtualTextureParameters` nodes carry the same names in the same order, which turns the
//! lookup into one archive read per distinct template.

use std::collections::HashMap;

use maclarian::formats::lsf::parse_lsf_bytes;

use crate::infrastructure::archives::{Archives, Pak};

/// The parameter every binding fills, keyed by material GUID, in binding order — for the materials
/// named by `materials`, each paired with its `SourceFile` template.
///
/// A template that cannot be read contributes an empty list: the names decorate a chip, and a missing
/// one must not fail a detail view.
pub fn read_parameters(
    pool: &mut Archives,
    materials: &[(String, String)],
) -> HashMap<String, Vec<String>> {
    let mut templates: HashMap<&str, Vec<String>> = HashMap::new();
    let mut parameters = HashMap::new();

    for (material_id, source_file) in materials {
        let names = template_parameters(pool, &mut templates, source_file.as_str());
        parameters.insert(material_id.clone(), names);
    }

    parameters
}

// The parameter names of `source_file`'s template, read at most once per template: templates are
// shared between materials, so the cache avoids re-reading the same file for each of them
fn template_parameters<'a>(
    pool: &mut Archives,
    templates: &mut HashMap<&'a str, Vec<String>>,
    source_file: &'a str,
) -> Vec<String> {
    match templates.get(source_file) {
        Some(names) => names.clone(),
        None => {
            let names = read_template(pool, source_file);
            templates.insert(source_file, names.clone());
            names
        }
    }
}

// Parameter names declared by one material template, in binding order. The nodes are read flat rather
// than through a bank region: a template holds a single material, and `VirtualTextureParameters` only
// occurs under it. Node order is document order — the very order maclarian reports the bindings in —
// which is what lets the two pair up by position.
fn read_template(pool: &mut Archives, source_file: &str) -> Vec<String> {
    // Read from `Materials.pak`, the one archive material templates ship in
    let Ok(bytes) = pool.read_from(Pak::Materials, source_file) else {
        return Vec::new();
    };
    let Ok(doc) = parse_lsf_bytes(&bytes) else {
        return Vec::new();
    };

    (0..doc.nodes.len())
        .filter(|&node| doc.node_name(node) == Some("VirtualTextureParameters"))
        .filter_map(|node| doc.get_fixed_string_attr(node, "ParameterName"))
        .collect()
}

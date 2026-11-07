//! Virtual texture parameter names, read from the LSF data.
//!
//! maclarian keeps only the `ID` of a `VirtualTextureParameters` node, so the parameter name is read
//! from the material's template (`MaterialDef::source_file`) rather than its `_merged.lsf` document:
//! templates carry the same names in the same order, one archive read per distinct template.

use std::collections::HashMap;

use maclarian::formats::lsf::parse_lsf_bytes;

use crate::archives::{Archives, Pak};

/// Parameter names keyed by material GUID, for the materials named by `materials`, each paired
/// with its `SourceFile` template. An unreadable template contributes an empty list.
pub fn read_parameters(
    pool: &mut Archives,
    materials: &[(String, String)],
) -> HashMap<String, Vec<String>> {
    let mut templates: HashMap<&str, Vec<String>> = HashMap::new();
    let mut parameters = HashMap::new();

    for (material_id, source_file) in materials {
        let names = match templates.get(source_file.as_str()) {
            Some(names) => names.clone(),
            None => {
                let names = read_template(pool, source_file);
                templates.insert(source_file.as_str(), names.clone());
                names
            }
        };
        parameters.insert(material_id.clone(), names);
    }

    parameters
}

/// Parameter names declared by one material template, in document order, so they pair with the
/// bindings by position.
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

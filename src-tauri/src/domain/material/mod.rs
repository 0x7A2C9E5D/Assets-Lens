//! What a material is, which resources it binds, and the rows the app lists it as — the material rows
//! of the detail panel and the entries of the export manifest.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod manifest;
mod summary;

pub use manifest::{manifest_materials, ExportMaterial};
pub(crate) use summary::fill_material_bindings;
pub use summary::{material_summaries, MaterialSummary};

/// One material of the built database: the name that makes its GUID readable, plus the resources it
/// binds (GUIDs, in parameter order).
///
/// `Clone` so a command can hand one asset's entries to an export task without copying the whole
/// cache. Serialized because the whole map is persisted next to the database it came out of
/// (`crate::application::cache`) — the names are not in the database itself, so they travel with it
#[derive(Clone, Serialize, Deserialize)]
pub struct MaterialInfo {
    /// Human-readable name from `MaterialBank` (e.g. `BEAR_Body_A`); empty when the resource has none
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// GUIDs of the textures this material binds
    pub texture_ids: Vec<String>,
    /// The virtual textures this material binds. An asset's virtual texture list is the union over
    /// its materials, so this is what tells those rows which material they came from.
    pub virtual_textures: Vec<VirtualTextureBinding>,
}

/// A virtual texture one material binds, together with the parameter that binding fills.
#[derive(Clone, Serialize, Deserialize)]
pub struct VirtualTextureBinding {
    /// GUID of the virtual texture resource (`VirtualTextureBank`)
    pub id: String,
    /// `ParameterName` of the binding node (e.g. `virtualtexture`). Empty until a detail view
    /// resolves it: maclarian keeps only the GUID of a binding, so the name is read off the
    /// material's template on demand (`ensure_virtual_texture_parameters`)
    pub parameter_name: String,
}

/// Fill in the parameter name of every virtual texture binding from the material's own template.
///
/// maclarian drops the names, so they are read separately (see `virtual_texture_params`) and joined
/// here. Bindings and names are paired by position, which holds because both come out in document
/// order. A material the reader did not reach keeps its empty names.
pub fn fill_virtual_texture_parameters(
    materials: &mut HashMap<String, MaterialInfo>,
    parameters: &HashMap<String, Vec<String>>,
) {
    for (material_id, names) in parameters {
        let Some(material) = materials.get_mut(material_id) else {
            continue;
        };

        for (binding, name) in material.virtual_textures.iter_mut().zip(names) {
            if binding.parameter_name.is_empty() {
                binding.parameter_name.clone_from(name);
            }
        }
    }
}

/// The entries of the material cache that `material_ids` reference, as an owned subset.
///
/// A material row of the export manifest reads the resources it binds out of these entries, and the
/// whole cache stays behind — one entry per material of the game would be far too much to hand over
/// (`manifest_materials`).
pub fn materials_of(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> HashMap<String, MaterialInfo> {
    material_ids
        .iter()
        .filter_map(|id| {
            materials
                .get(id)
                .map(|material| (id.clone(), material.clone()))
        })
        .collect()
}

// The asset's rows indexed by GUID, so a material's references can be looked up without a scan. Both
// a material row of the detail panel and one of the export manifest join on the GUID, which is what
// keeps two same-named materials apart. Keyed by `&str` because the rows outlive the map and the ids
// are only ever compared.
fn rows_by_id<'a, T>(rows: &'a [T], id_of: impl Fn(&'a T) -> &'a str) -> HashMap<&'a str, &'a T> {
    rows.iter().map(|row| (id_of(row), row)).collect()
}

// Parameter the asset's materials bind the virtual texture `id` with. The name belongs to the binding
// rather than to the resource, so it is taken from the first material that binds it — in the shipped
// data a virtual texture is bound once and with the same name everywhere.
pub(crate) fn virtual_texture_parameter(
    id: &str,
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> Option<String> {
    material_ids
        .iter()
        .filter_map(|material_id| materials.get(material_id))
        .flat_map(|material| material.virtual_textures.as_slice())
        .find(|binding| binding.id == id)
        .map(|binding| binding.parameter_name.clone())
        .filter(|name| !name.is_empty())
}

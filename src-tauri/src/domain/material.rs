//! What a material is and which resources it binds.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// One material of the built database: the name that makes its GUID readable, plus the resources it
/// binds (GUIDs, in parameter order).
///
/// `Clone` so a command can hand the entries of one asset to an export task without copying the whole
/// cache (see `materials_of`). Serialized because the whole map is persisted to disk with the
/// database it came out of (`crate::cache`), which is what saves a build on the next launch: the names
/// are not in the database itself (see `crate::state::extract_materials`), so they have to travel with
/// it.
#[derive(Clone, Serialize, Deserialize)]
pub struct MaterialInfo {
    /// Human-readable name from `MaterialBank` (e.g. `BEAR_Body_A`); empty when the resource has none
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// GUIDs of the textures this material binds
    pub texture_ids: Vec<String>,
    /// The virtual textures this material binds. An asset's virtual texture list is the union over
    /// its materials, so this is what tells those rows which material they came from: a virtual
    /// texture is only ever reachable through the material that parameterizes it.
    pub virtual_textures: Vec<VirtualTextureBinding>,
}

/// A virtual texture one material binds, together with the parameter that binding fills.
#[derive(Clone, Serialize, Deserialize)]
pub struct VirtualTextureBinding {
    /// GUID of the virtual texture resource (`VirtualTextureBank`)
    pub id: String,
    /// `ParameterName` of the binding node (e.g. `virtualtexture`, `overlayvirtualtexture`). Empty
    /// until a detail view resolves it: maclarian keeps only the GUID of a binding
    /// (see `virtual_texture_params`), so the name is read off the material's template on demand
    /// (`commands::ensure_virtual_texture_parameters`) and stays empty only until then.
    pub parameter_name: String,
}

/// Fill in the parameter name of every virtual texture binding from the material's own template.
///
/// The names are read separately from the database — maclarian drops them, and reading them is a
/// per-detail-view job (see `virtual_texture_params`) — so the two are joined here. Bindings and
/// names are paired by position, which holds because both come out in document order. A material the
/// reader did not reach keeps its empty names, and the panel then renders the chip without one.
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
/// A material row of the export manifest reads the resources it binds out of these entries (see
/// `crate::models::manifest_materials`), and the cache they live in is not handed to the export task
/// — so the export takes the handful of entries it needs rather than the whole cache (one entry per
/// material of the game, each holding its own texture lists).
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

/// Parameter the asset's materials bind the virtual texture `id` with.
///
/// The name belongs to the binding rather than to the resource, so it is taken from the first
/// material that binds it — in the shipped data a virtual texture is bound once and with the same
/// name everywhere, which is what makes one name per row enough.
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
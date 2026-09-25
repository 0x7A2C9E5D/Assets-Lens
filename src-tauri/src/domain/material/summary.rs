//! The material rows of the detail panel: what a material references, and the resources it binds.

use std::collections::HashMap;

use serde::Serialize;

use super::{rows_by_id, MaterialInfo, VirtualTextureBinding};
use crate::domain::source::{source_of, ModSources};
use crate::domain::texture::TextureSummary;
use crate::domain::virtual_textures::VirtualTextureSummary;

/// Which list of the asset a material binding points into
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingKind {
    /// A regular texture (`TextureSummary` row)
    Texture,
    /// A streaming virtual texture (`VirtualTextureSummary` row)
    Virtual,
}

/// One resource a material binds, as the detail panel lists it under that material.
///
/// The relation is stated per material and keyed by GUID: a material name is not unique, so inverting
/// a texture list by name would mix up same-named materials and drop an unnamed one's bindings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialBinding {
    pub kind: BindingKind,
    /// GUID of the resource, the same value its row in the asset's lists carries
    pub id: String,
    /// Name of that row; empty when the resource has none, and the panel falls back to the GUID
    pub name: String,
    /// Parameter the binding fills (e.g. `virtualtexture`), for a virtual texture. Taken from this
    /// material's own binding: the name belongs to the binding, not to the resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
}

/// Material reference. The GUID stays the identity (names are not unique), the name is what makes
/// a material row readable, and the template file is where the material is defined.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSummary {
    pub id: String,
    /// Human-readable name from `MaterialBank`; empty when the material is unknown, and the detail
    /// panel then falls back to the GUID
    pub name: String,
    /// Mod providing this material; absent for the game's own
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// The resources this material binds, in binding order. Left out of the JSON while empty, which
    /// is what a material the cache does not know yields.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<MaterialBinding>,
}

impl MaterialSummary {
    // `known` is the name cache entry for this GUID; an unknown material keeps an empty name rather than
    // dropping the row. Bindings are filled later, once the resource rows exist
    pub(super) fn new(id: &str, known: Option<&MaterialInfo>, sources: &ModSources) -> Self {
        Self {
            id: id.to_string(),
            name: known
                .map(|material| material.name.clone())
                .unwrap_or_default(),
            source: source_of(sources, id),
            source_file: known
                .map(|material| material.source_file.clone())
                .unwrap_or_default(),
            bindings: Vec::new(),
        }
    }
}

/// Summaries of `material_ids` in the order the asset lists them: the material rows of the detail
/// panel. A material is identified by its GUID but only readable through the name, so a material the
/// cache does not know keeps an empty name instead of dropping out of the list.
pub fn material_summaries(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
) -> Vec<MaterialSummary> {
    material_ids
        .iter()
        .map(|id| MaterialSummary::new(id, materials.get(id), sources))
        .collect()
}

// Fill in, for every material row, the resources of the asset that material binds. A material names
// its resources by GUID, so each one is looked up in the asset's own rows and joined on the GUID
// rather than on the name — which is what keeps two same-named materials apart and still shows what an
// unnamed one binds. A material the cache does not know keeps an empty binding list.
pub(crate) fn fill_material_bindings(
    materials: &mut [MaterialSummary],
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    known: &HashMap<String, MaterialInfo>,
) {
    let textures = rows_by_id(textures, |row| row.id.as_str());
    let virtual_textures = rows_by_id(virtual_textures, |row| row.id.as_str());

    for row in materials.iter_mut() {
        row.bindings = material_bindings(known.get(&row.id), &textures, &virtual_textures);
    }
}

// What one material binds, in binding order: its textures first, then its virtual textures. A
// material the cache does not know binds anything.
fn material_bindings(
    known: Option<&MaterialInfo>,
    textures: &HashMap<&str, &TextureSummary>,
    virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
) -> Vec<MaterialBinding> {
    let Some(material) = known else {
        return Vec::new();
    };
    let mut bindings = texture_bindings(material, textures);
    bindings.extend(virtual_texture_bindings(material, virtual_textures));
    bindings
}

// The textures a material binds, in the material's own (parameter) order. A resource no asset row
// carries contributes nothing.
fn texture_bindings(
    material: &MaterialInfo,
    textures: &HashMap<&str, &TextureSummary>,
) -> Vec<MaterialBinding> {
    material
        .texture_ids
        .iter()
        .filter_map(|id| textures.get(id.as_str()).copied())
        .map(texture_binding)
        .collect()
}

// The binding row of one texture the material references
fn texture_binding(texture: &TextureSummary) -> MaterialBinding {
    MaterialBinding {
        kind: BindingKind::Texture,
        id: texture.id.clone(),
        name: texture.name.clone(),
        parameter_name: texture.parameter_name.clone(),
    }
}

// The virtual textures a material binds, in binding order. A resource no asset row carries
// contributes nothing.
fn virtual_texture_bindings(
    material: &MaterialInfo,
    virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
) -> Vec<MaterialBinding> {
    material
        .virtual_textures
        .iter()
        .filter_map(|binding| virtual_texture_binding(binding, virtual_textures))
        .collect()
}

// The binding row of one virtual texture the material references, or `None` when no asset row carries
// it. The parameter name comes from this material's own binding — the name belongs to the binding, not
// to the resource — and an empty one is left out.
fn virtual_texture_binding(
    binding: &VirtualTextureBinding,
    virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
) -> Option<MaterialBinding> {
    let row = virtual_textures.get(binding.id.as_str())?;
    Some(MaterialBinding {
        kind: BindingKind::Virtual,
        id: row.id.clone(),
        name: row.name.clone(),
        parameter_name: Some(binding.parameter_name.clone()).filter(|name| !name.is_empty()),
    })
}

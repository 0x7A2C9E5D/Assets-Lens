//! The material rows of the detail panel: what a material references, and the resources it binds.

use std::collections::HashMap;

use serde::Serialize;

use super::MaterialInfo;
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
/// a texture list by name mixes up same-named materials and drops the bindings of an unnamed one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialBinding {
    pub kind: BindingKind,
    /// GUID of the resource, the same value its row in the asset's lists carries
    pub id: String,
    /// Name of that row; empty when the resource has none, and the panel falls back to the GUID
    pub name: String,
    /// Parameter the binding fills (e.g. `virtualtexture`), for a virtual texture. Taken from this
    /// material's own binding rather than the resource: the name belongs to the binding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
}

/// Material reference. The GUID stays the identity (names are not unique), the name is what makes
/// a material row readable, and the template file is where the material is defined.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSummary {
    pub id: String,
    /// Human-readable name from `MaterialBank`; empty when the material is unknown, in which case
    /// the detail panel falls back to the GUID
    pub name: String,
    /// Mod providing this material; absent when it comes from the game (see `domain::source::ModSources`)
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
    /// `known` is the name cache entry for this GUID; an unknown material keeps an empty name
    /// rather than dropping the row. The bindings are filled later, once the asset's resource rows
    /// exist to read them off (see `fill_material_bindings`).
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

/// Summaries of `material_ids` in the order the asset lists them.
///
/// The material rows of the detail panel; the export manifest lists the same identity plus the
/// resources each material binds (see `ExportMaterial`). A material is identified by its GUID but is
/// only readable through the name, and a list carrying bare GUIDs cannot be looked up in the game
/// data. A material the cache does not know keeps an empty name instead of dropping out of the list,
/// so the reference itself is never lost.
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

/// Fill in, for every material row, the resources of the asset that material binds.
///
/// A material names its resources by GUID (see `MaterialInfo`), so each one is looked up in the
/// asset's own rows — the same rows the detail panel lists beside the material. Both sides are joined
/// on the GUID rather than on the name, which is what keeps two same-named materials apart and lets an
/// unnamed one still show what it binds. A resource a row does not list contributes anything, and a
/// material the cache does not know keeps an empty binding list.
pub(crate) fn fill_material_bindings(
    materials: &mut [MaterialSummary],
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    known: &HashMap<String, MaterialInfo>,
) {
    let textures: HashMap<&str, &TextureSummary> =
        textures.iter().map(|row| (row.id.as_str(), row)).collect();
    let virtual_textures: HashMap<&str, &VirtualTextureSummary> = virtual_textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();

    for row in materials.iter_mut() {
        let Some(material) = known.get(&row.id) else {
            continue;
        };

        row.bindings = material
            .texture_ids
            .iter()
            .filter_map(|id| textures.get(id.as_str()))
            .map(|texture| MaterialBinding {
                kind: BindingKind::Texture,
                id: texture.id.clone(),
                name: texture.name.clone(),
                parameter_name: texture.parameter_name.clone(),
            })
            .chain(material.virtual_textures.iter().filter_map(|binding| {
                let row = virtual_textures.get(binding.id.as_str())?;
                Some(MaterialBinding {
                    kind: BindingKind::Virtual,
                    id: row.id.clone(),
                    name: row.name.clone(),
                    // Taken from this material's own binding: the parameter belongs to the binding,
                    // not to the resource, so the row-level one would only be an approximation
                    parameter_name: Some(binding.parameter_name.clone())
                        .filter(|name| !name.is_empty()),
                })
            }))
            .collect();
    }
}

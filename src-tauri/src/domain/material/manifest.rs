//! The material rows of the export manifest: the identity of a `MaterialSummary` plus the resources
//! each material binds, stated in full for a reader that only has the manifest.

use std::collections::HashMap;

use maclarian::merged::VisualAsset;
use serde::Serialize;

use super::rows_by_id;
use super::summary::MaterialSummary;
use super::MaterialInfo;
use crate::domain::source::ModSources;
use crate::domain::texture::TextureSummary;
use crate::domain::virtual_textures::VirtualTextureSummary;

/// One material as `asset.json` lists it: the identity of a `MaterialSummary` plus the resources the
/// material binds, each stated in full.
///
/// A row carries whole resource rows (name, path, size, parameter) instead of GUIDs to look up, so a
/// reader takes the resources of a material from one place. The reverse statement of a detail row
/// (`bindings`) is left out: a material containing its textures says that already.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMaterial {
    pub id: String,
    pub name: String,
    /// Mod providing this material; absent for the game's own
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub source_file: String,
    /// The textures this material binds, in parameter order
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<TextureSummary>,
    /// The virtual textures this material binds, in binding order
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl ExportMaterial {
    // `known` is the name cache entry for this GUID. A material names its resources by GUID, so
    // `textures` / `virtual_textures` are looked up in the asset's own rows by that GUID.
    fn new(
        id: &str,
        known: Option<&MaterialInfo>,
        textures: &HashMap<&str, &TextureSummary>,
        virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
        sources: &ModSources,
    ) -> Self {
        Self::from_summary(
            MaterialSummary::new(id, known, sources),
            known,
            textures,
            virtual_textures,
        )
    }

    // The manifest row for a material whose identity is already stated: an unknown material keeps an
    // empty name here exactly as it does on a detail row, since the identity comes from the summary.
    fn from_summary(
        summary: MaterialSummary,
        known: Option<&MaterialInfo>,
        textures: &HashMap<&str, &TextureSummary>,
        virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
    ) -> Self {
        Self {
            id: summary.id,
            name: summary.name,
            source: summary.source,
            source_file: summary.source_file,
            textures: bound_textures(known, textures),
            virtual_textures: bound_virtual_textures(known, virtual_textures),
        }
    }
}

// The asset's texture rows for the textures a material binds, in the material's own (parameter)
// order. An id the rows do not carry contributes anything.
fn bound_textures(
    known: Option<&MaterialInfo>,
    textures: &HashMap<&str, &TextureSummary>,
) -> Vec<TextureSummary> {
    let Some(material) = known else {
        return Vec::new();
    };
    material
        .texture_ids
        .iter()
        .filter_map(|id| textures.get(id.as_str()))
        .map(|row| (*row).clone())
        .collect()
}

// The asset's virtual texture rows for the resources a material binds, in binding order. An id the
// rows do not carry contributes anything.
fn bound_virtual_textures(
    known: Option<&MaterialInfo>,
    virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
) -> Vec<VirtualTextureSummary> {
    let Some(material) = known else {
        return Vec::new();
    };
    material
        .virtual_textures
        .iter()
        .filter_map(|binding| virtual_textures.get(binding.id.as_str()))
        .map(|row| (*row).clone())
        .collect()
}

/// The materials of `value` in the asset's own order, as the export manifest lists them: each one
/// carrying the asset's texture and virtual texture rows for the resources it binds.
///
/// Those rows are the manifest's only statement of the asset's resources, and they are whole because
/// the manifest is read on its own — a GUID to look up would not do.
pub fn manifest_materials(
    value: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    sources: &ModSources,
) -> Vec<ExportMaterial> {
    let textures = rows_by_id(textures, |row| row.id.as_str());
    let virtual_textures = rows_by_id(virtual_textures, |row| row.id.as_str());

    value
        .material_ids
        .iter()
        .map(|id| ExportMaterial::new(id, materials.get(id), &textures, &virtual_textures, sources))
        .collect()
}

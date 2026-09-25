//! The material rows of the export manifest: the identity of a `MaterialSummary` plus the resources
//! each material binds, stated in full for a reader that only has the manifest.

use std::collections::HashMap;

use maclarian::merged::VisualAsset;
use serde::Serialize;

use super::summary::MaterialSummary;
use super::MaterialInfo;
use crate::domain::source::ModSources;
use crate::domain::texture::TextureSummary;
use crate::domain::virtual_textures::VirtualTextureSummary;

/// One material as `asset.json` lists it: the identity of a `MaterialSummary` plus the resources the
/// material binds, each stated in full.
///
/// A material row carries the asset's texture rows for its own resources, in binding order: each
/// resource in full (name, path, size, parameter) rather than a GUID to look up, so a reader takes the
/// resources of a material from one place instead of joining two lists by GUID. What a row of the
/// detail panel states the other way round (its `bindings`) is left out here: a material containing
/// its textures says that already.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMaterial {
    pub id: String,
    pub name: String,
    /// Mod providing this material; absent when it comes from the game (see `domain::source::ModSources`)
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
    /// `known` is the name cache entry for this GUID. The identity is taken from `MaterialSummary`, so
    /// an unknown material keeps an empty name here exactly as it does on a detail row.
    ///
    /// A material names its resources by GUID, so `textures` / `virtual_textures` are looked up in the
    /// asset's own rows by that GUID.
    fn new(
        id: &str,
        known: Option<&MaterialInfo>,
        textures: &HashMap<&str, &TextureSummary>,
        virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
        sources: &ModSources,
    ) -> Self {
        let MaterialSummary {
            id,
            name,
            source,
            source_file,
            // The manifest states the resources inside each material, so the row-level bindings of
            // the detail panel have nothing to add here
            bindings: _,
        } = MaterialSummary::new(id, known, sources);

        Self {
            id,
            name,
            source,
            source_file,
            // In the material's own order (its parameter order); an id the asset's texture rows do
            // not carry contributes anything
            textures: known
                .map(|material| {
                    material
                        .texture_ids
                        .iter()
                        .filter_map(|id| textures.get(id.as_str()))
                        .map(|row| (*row).clone())
                        .collect()
                })
                .unwrap_or_default(),
            virtual_textures: known
                .map(|material| {
                    material
                        .virtual_textures
                        .iter()
                        .filter_map(|binding| virtual_textures.get(binding.id.as_str()))
                        .map(|row| (*row).clone())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// The materials of `value` in the asset's own order, as the export manifest lists them: each one
/// carrying the asset's texture and virtual texture rows for the resources it binds.
///
/// Those rows are the manifest's only statement of the asset's resources — the material containing them
/// says which material they belong to, so a separate list beside the materials would only repeat it.
/// This is also why the rows are whole: the manifest is read on its own, so a GUID to look up would
/// not do.
pub fn manifest_materials(
    value: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    sources: &ModSources,
) -> Vec<ExportMaterial> {
    let textures: HashMap<&str, &TextureSummary> =
        textures.iter().map(|row| (row.id.as_str(), row)).collect();
    let virtual_textures: HashMap<&str, &VirtualTextureSummary> = virtual_textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();

    value
        .material_ids
        .iter()
        .map(|id| ExportMaterial::new(id, materials.get(id), &textures, &virtual_textures, sources))
        .collect()
}

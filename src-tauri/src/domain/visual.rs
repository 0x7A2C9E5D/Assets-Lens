//! Visual assets: the list row, the detail payload of the detail panel, and the model preview.
//!
//! The detail payload is where the three resource lists of an asset meet: the material rows are the
//! only place the relation is stated (see `material::fill_material_bindings`), so this is the one
//! builder that has all three at hand.

use std::collections::HashMap;

use maclarian::merged::{GtpMatch, VisualAsset};
use serde::Serialize;

use crate::domain::material::{
    fill_material_bindings, material_summaries, MaterialInfo, MaterialSummary,
};
use crate::domain::source::{source_of, ModSources};
use crate::domain::texture::{texture_summaries, TextureSummary};
use crate::domain::virtual_textures::{virtual_texture_summaries, VirtualTextureSummary};

/// Full visual asset information (detail panel)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualAssetDetail {
    /// Visual resource ID (GUID) — the lookup key, since names are not unique
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    pub materials: Vec<MaterialSummary>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl VisualAssetDetail {
    /// `matches` are the page files resolved for this asset's virtual textures; pass an empty
    /// slice to skip the lookup (the rows then show the hash without a page file).
    /// `materials` is the name cache built with the database: it labels the material rows and lists,
    /// per material, the resources of this asset that material binds (see `fill_material_bindings`).
    /// `sources` labels every row with the mod that provides it, and leaves the game's own rows
    /// unlabeled.
    pub fn new(
        value: &VisualAsset,
        matches: &[GtpMatch],
        materials: &HashMap<String, MaterialInfo>,
        sources: &ModSources,
    ) -> Self {
        // Built in this order because the material rows end up listing their own resources: the
        // bindings are read off the two resource lists, so those have to exist first
        let mut material_rows = material_summaries(&value.material_ids, materials, sources);
        let textures = texture_summaries(value, sources);
        let virtual_textures = virtual_texture_summaries(value, matches, materials, sources);
        fill_material_bindings(&mut material_rows, &textures, &virtual_textures, materials);

        // One line over the limit, deliberately: the structure of the payload is the point here, and
        // moving any field into a helper of its own would only hide which value it names.
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            source: source_of(sources, &value.id),
            path: value.gr2_path.clone(),
            materials: material_rows,
            textures,
            virtual_textures,
        }
    }
}

/// 3D preview payload: the GR2 mesh converted to GLB.
/// Stored as a Base64 string instead of `Vec<u8>`: a byte vector would serialize through serde into
/// a JSON array of numbers (one per byte), inflating a multi-MB model to tens of MB of JSON;
/// Base64 only grows by ~33% and keeps everything in memory without temp files
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreview {
    pub base64: String,
}

/// Visual asset list item: highlights the composition of the visual itself
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSummary {
    /// Visual resource ID (GUID): the row identity — one name can belong to several visuals, so the
    /// list cannot be keyed by name
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

impl VisualSummary {
    pub fn of(value: &VisualAsset, sources: &ModSources) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            source: source_of(sources, &value.id),
            material_count: value.material_ids.len(),
            texture_count: value.textures.len(),
            virtual_texture_count: value.virtual_textures.len(),
        }
    }
}

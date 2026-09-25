//! Textures: a row of the asset's texture list.

use maclarian::merged::{TextureRef, VisualAsset};
use serde::Serialize;

use crate::domain::source::{source_of, ModSources};

/// Texture reference (DDS).
///
/// A row of the asset's texture list (the detail panel), and — copied — the entry the export manifest
/// material binding it carries: the resource stated in full (name, path, size, parameter) rather than
/// a GUID to look up. Hence, `Clone`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    /// Mod providing this texture; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
}

impl From<&TextureRef> for TextureSummary {
    fn from(value: &TextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            // Settled by the caller, which is the only place holding the source map
            source: None,
            path: value.dds_path.clone(),
            width: value.width,
            height: value.height,
            parameter_name: value.parameter_name.clone(),
        }
    }
}

/// The textures of `value`, in the asset's own order.
///
/// The texture list of the detail panel, and the source the export manifest builds its material rows
/// from (see `material::manifest_materials`). Which material binds a texture is stated the other way
/// round, on the material rows (see `material::fill_material_bindings`).
pub fn texture_summaries(value: &VisualAsset, sources: &ModSources) -> Vec<TextureSummary> {
    value
        .textures
        .iter()
        .map(|texture| {
            let mut summary = TextureSummary::from(texture);
            summary.source = source_of(sources, &texture.id);
            summary
        })
        .collect()
}

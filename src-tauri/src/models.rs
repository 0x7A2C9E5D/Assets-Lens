use std::collections::HashMap;

use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use serde::{Deserialize, Serialize};

use crate::state::MaterialInfo;

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub percent: f32,
}

/// Texture reference (DDS)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Archive holding this DDS (e.g. `Textures.pak`); left empty here and filled in by `get_visual`,
    /// which is the only place with access to the PAK pool — maclarian never fills
    /// `TextureRef::source_pak`, so the export manifest still carries it empty
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
    /// Names of this asset's materials that bind this texture, in the asset's own material order.
    /// Filled by `get_visual` only, and left out of the JSON while empty (the export manifest
    /// resolves no names). The texture row no longer renders this — a texture shows its own `name`
    /// — so it is currently unused by the panel and kept for the material section.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub material_names: Vec<String>,
}

impl From<&TextureRef> for TextureSummary {
    fn from(value: &TextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.dds_path.clone(),
            source: String::new(),
            width: value.width,
            height: value.height,
            parameter_name: value.parameter_name.clone(),
            material_names: Vec::new(),
        }
    }
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
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
}

impl MaterialSummary {
    /// `known` is the name cache entry for this GUID; an unknown material keeps an empty name
    /// rather than dropping the row
    fn new(id: &str, known: Option<&MaterialInfo>) -> Self {
        Self {
            id: id.to_string(),
            name: known.map(|material| material.name.clone()).unwrap_or_default(),
            source_file: known
                .map(|material| material.source_file.clone())
                .unwrap_or_default(),
        }
    }
}

/// Names of the materials of one asset that bind `texture`. More than one is possible (two
/// materials of the same mesh may share a mask), so this is a list; a material with no name
/// contributes nothing, because a bare GUID would only repeat what the material section already
/// shows.
fn material_names_for(
    texture: &TextureRef,
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> Vec<String> {
    material_ids
        .iter()
        .filter_map(|id| materials.get(id))
        .filter(|material| material.texture_ids.iter().any(|id| id == &texture.id))
        .map(|material| material.name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Streaming virtual texture reference (GTex)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
    pub hash: String,
    /// Page file (`.gtp`) inside its archive; empty when no lookup was run or nothing matched
    pub path: String,
    /// Archive holding that page file, taken from the match itself (`GtpMatch::pak_path`) rather
    /// than rebuilt from a configured archive name — unresolved hashes stay empty instead of
    /// claiming a plausible-but-unverified archive
    pub source: String,
    /// Pixel size of this page file, read out of its tile set's GTS on demand by `get_visual` — the
    /// same box the extractor writes as its DDS. `None` when the hash resolved to no page file or
    /// that GTS could not be parsed (the row then renders without a size); the export manifest
    /// leaves it `None` because it never reads the GTS
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl VirtualTextureSummary {
    /// `matched` is the page file resolved for this hash — `None` when the lookup found nothing
    /// (in which case the row renders without path and archive) or was skipped by the caller
    pub fn new(value: &VirtualTextureRef, matched: Option<&GtpMatch>) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            hash: value.gtex_hash.clone(),
            path: matched.map(|m| m.gtp_path.clone()).unwrap_or_default(),
            source: matched
                .and_then(|m| m.pak_path.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            // Settled later: reading the size needs the archives, which this constructor is not given
            width: None,
            height: None,
        }
    }

    /// Fill in the page file size. A failed lookup leaves both fields unset: the size is
    /// decoration like the archive name, and must not take a detail view or an export down with it.
    pub fn set_size(&mut self, size: Option<(u32, u32)>) {
        if let Some((width, height)) = size {
            self.width = Some(width);
            self.height = Some(height);
        }
    }
}

/// The page file resolved for one GTex hash. `GtpMatch::gtex_hash` echoes the hash that was
/// searched for, so this is a plain lookup rather than a hash comparison.
pub fn match_for_hash<'a>(matches: &'a [GtpMatch], hash: &str) -> Option<&'a GtpMatch> {
    let hash = hash.trim();
    matches
        .iter()
        .find(|matched| matched.gtex_hash.eq_ignore_ascii_case(hash))
}

/// Full visual asset information (detail panel)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualAssetDetail {
    /// Visual resource ID (GUID) — the lookup key, since names are not unique
    pub id: String,
    pub name: String,
    pub path: String,
    /// Archive holding the GR2 mesh (e.g. `Models.pak`); left empty here and filled in by
    /// `get_visual`, which is the only place with access to the PAK pool
    pub mesh_pak: String,
    pub materials: Vec<MaterialSummary>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl VisualAssetDetail {
    /// `matches` are the page files resolved for this asset's virtual textures; pass an empty
    /// slice to skip the lookup (the rows then show the hash without a page file).
    /// `materials` is the name cache built with the database: it labels the material rows and
    /// decides which material each texture row belongs to
    pub fn new(
        value: &VisualAsset,
        matches: &[GtpMatch],
        materials: &HashMap<String, MaterialInfo>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.gr2_path.clone(),
            mesh_pak: String::new(),
            materials: value
                .material_ids
                .iter()
                .map(|id| MaterialSummary::new(id, materials.get(id)))
                .collect(),
            textures: value
                .textures
                .iter()
                .map(|texture| {
                    let mut summary = TextureSummary::from(texture);
                    summary.material_names =
                        material_names_for(texture, &value.material_ids, materials);
                    summary
                })
                .collect(),
            virtual_textures: value
                .virtual_textures
                .iter()
                .map(|vt| VirtualTextureSummary::new(vt, match_for_hash(matches, &vt.gtex_hash)))
                .collect(),
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
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

impl From<&VisualAsset> for VisualSummary {
    fn from(value: &VisualAsset) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            material_count: value.material_ids.len(),
            texture_count: value.textures.len(),
            virtual_texture_count: value.virtual_textures.len(),
        }
    }
}

/// Database statistics
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    pub visual_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

/// App metadata for the About page
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Application version (compile-time, kept in sync with tauri.conf.json)
    pub version: String,
}

/// Generic pagination result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
}

/* ---------- Asset export ---------- */

/// Mesh export format: the raw GR2 out of the archive, or a converted GLB
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MeshFormat {
    /// Raw GR2 bytes, straight out of the PAK (no conversion)
    #[default]
    Gr2,
    /// Converted GLB mesh; textures are exported separately
    Glb,
}

impl MeshFormat {
    pub fn is_glb(self) -> bool {
        matches!(self, Self::Glb)
    }

    /// File extension of the mesh artifact
    pub fn extension(self) -> &'static str {
        match self {
            Self::Gr2 => "gr2",
            Self::Glb => "glb",
        }
    }
}

/// Texture export format: no separate textures, DDS, or PNG.
/// Applies to both regular textures and virtual textures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextureFormat {
    /// Do not export texture files separately
    None,
    /// Export as DDS (keep the archive format)
    #[default]
    Dds,
    /// Export as PNG (discard DDS after successful conversion)
    Png,
}

impl TextureFormat {
    pub fn is_export(self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn is_png(self) -> bool {
        matches!(self, Self::Png)
    }
}

/// Export options (submitted from the frontend)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Mesh output format: raw GR2 or converted GLB
    #[serde(default)]
    pub mesh_format: MeshFormat,
    /// Texture output format for separate files — covers both regular textures and virtual textures
    #[serde(default)]
    pub texture_format: TextureFormat,
}

/// Export progress, pushed through a Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// Phase: prepare / model / textures / virtualTextures / manifest / done
    pub phase: String,
    pub current_file: Option<String>,
    pub percent: f32,
}

/// One exported artifact
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    /// Artifact kind: gr2 / glb / dds / png
    pub kind: String,
    pub size_bytes: usize,
}

/// Export warning: `code` is a stable enum (mapped to i18n copy on the frontend) while `detail`
/// carries the raw detail (path / error); the frontend shows `detail` directly for unknown codes,
/// so warnings passed through from maclarian never end up as a missing-copy gap
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub code: String,
    pub detail: String,
}

/// Export result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// The directory actually written to (destDir/<asset name>)
    pub output_dir: String,
    pub files: Vec<ExportedFile>,
    /// Non-fatal issues: missing textures, failed PNG conversions, etc.
    pub warnings: Vec<ExportWarning>,
}

/// Content of asset.json
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    pub name: String,
    pub path: String,
    /// Mesh format that was exported: gr2 / glb
    pub mesh_format: MeshFormat,
    pub source: String,
    pub material_ids: Vec<String>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
    /// Exported artifacts; asset.json itself is deliberately not listed here
    pub files: Vec<ExportedFile>,
    pub exported_at_unix: u64,
    pub maclarian_version: String,
}

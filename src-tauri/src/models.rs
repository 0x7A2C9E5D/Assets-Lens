use maclarian::merged::{TextureRef, VirtualTextureRef, VisualAsset};
use serde::{Deserialize, Serialize};

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub current: usize,
    pub total: usize,
    pub current_file: Option<String>,
    pub percent: f32,
}

/// Texture reference (DDS)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
}

impl From<&TextureRef> for TextureSummary {
    fn from(value: &TextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.dds_path.clone(),
            source: value.source_pak.clone(),
            width: value.width,
            height: value.height,
            parameter_name: value.parameter_name.clone(),
        }
    }
}

/// Streaming virtual texture reference (GTex)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
    pub hash: String,
}

impl From<&VirtualTextureRef> for VirtualTextureSummary {
    fn from(value: &VirtualTextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            hash: value.gtex_hash.clone(),
        }
    }
}

/// Full visual asset information (detail panel)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualAssetDetail {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub material_ids: Vec<String>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl From<&VisualAsset> for VisualAssetDetail {
    fn from(value: &VisualAsset) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.gr2_path.clone(),
            source: value.source_pak.clone(),
            material_ids: value.material_ids.clone(),
            textures: value.textures.iter().map(TextureSummary::from).collect(),
            virtual_textures: value.virtual_textures.iter().map(VirtualTextureSummary::from).collect(),
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
    pub name: String,
    pub path: String,
    pub source: String,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

impl From<&VisualAsset> for VisualSummary {
    fn from(value: &VisualAsset) -> Self {
        Self {
            name: value.name.clone(),
            path: value.gr2_path.clone(),
            source: value.source_pak.clone(),
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

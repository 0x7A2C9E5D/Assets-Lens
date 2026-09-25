//! What an export is asked for and what it reports back: the formats, the progress, the artifacts
//! written, and the `asset.json` manifest that describes the asset.
//!
//! The workflow itself is `crate::infrastructure::export`; these are the shapes it takes in and
//! hands out.

use serde::{Deserialize, Serialize};

use crate::domain::material::ExportMaterial;

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
    #[serde(default)]
    pub mesh_format: MeshFormat,
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

/// Export warning: `code` is a stable enum (mapped to i18n copy on the frontend), while `detail`
/// carries the raw path or error and is shown as-is for unknown codes — so a warning passed through
/// from maclarian never ends up as a missing-copy gap
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

/// Content of asset.json: the asset and the resources it is made of — not a listing of the export
/// directory. Which files a run wrote is the export result's business (`ExportResult`)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    /// Visual resource GUID: the asset's identity and lookup key, since the name is not unique
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent for the game's own
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    /// The asset's materials, in its own order. The manifest states the asset's resources here and
    /// nowhere else, so this is the list to read them from
    pub materials: Vec<ExportMaterial>,
    pub exported_at_unix: u64,
    pub maclarian_version: String,
}

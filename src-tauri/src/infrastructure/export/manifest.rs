//! The manifest step: assembles the `asset.json` of one export and writes it next to the artifacts,
//! where a failure is a warning rather than the end of the export.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use maclarian::merged::{GtpMatch, VisualAsset};

use super::push_warning;
use crate::domain::export::{ExportManifest, ExportWarning};
use crate::domain::material::{manifest_materials, MaterialInfo};
use crate::domain::source::{source_of, ModSources};
use crate::domain::texture::texture_summaries;
use crate::domain::virtual_textures::virtual_texture_summaries;

/// Assemble the `asset.json` content of one export. The virtual texture sizes are the one field that
/// needs the archives, so they are settled by `fill_vt_sizes` before the manifest is written.
pub(super) fn build_manifest(
    asset: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
    vt_matches: &[GtpMatch],
) -> ExportManifest {
    // Built once and handed to the material rows, which carry these very rows for their own resources
    // (see `manifest_materials`); the manifest lists the resources nowhere else
    let textures = texture_summaries(asset, sources);
    let virtual_textures = virtual_texture_summaries(asset, vt_matches, materials, sources);

    ExportManifest {
        id: asset.id.clone(),
        name: asset.name.clone(),
        source: source_of(sources, &asset.id),
        path: asset.gr2_path.clone(),
        materials: manifest_materials(asset, materials, &textures, &virtual_textures, sources),
        exported_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        maclarian_version: maclarian::VERSION.to_string(),
    }
}

/// Write `asset.json`; the artifacts are already on disk by then, so a manifest failure is recorded
/// as a warning instead of failing the whole export
pub(super) fn write_manifest(
    manifest: &ExportManifest,
    out_dir: &Path,
    warnings: &mut Vec<ExportWarning>,
) {
    let manifest_path = out_dir.join("asset.json");
    match serde_json::to_string_pretty(manifest) {
        Ok(json) => {
            if let Err(err) = fs::write(&manifest_path, json) {
                push_warning(warnings, "manifestWriteFailed", err.to_string());
            }
        }
        Err(err) => push_warning(warnings, "manifestSerializeFailed", err.to_string()),
    }
}

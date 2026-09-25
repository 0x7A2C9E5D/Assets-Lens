//! The manifest step: assembles the `asset.json` of one export and writes it next to the artifacts,
//! where a failure is a warning rather than the end of the export.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use maclarian::merged::{GtpMatch, VisualAsset};

use super::ExportOutput;
use crate::domain::export::ExportManifest;
use crate::domain::material::{manifest_materials, MaterialInfo};
use crate::domain::source::{source_of, ModSources};
use crate::domain::texture::texture_summaries;
use crate::domain::virtual_textures::virtual_texture_summaries;

// Assemble the `asset.json` content of one export.
//
// The resource rows are built once and handed to the material rows, which carry these very rows for
// their own resources (see `manifest_materials`); the manifest lists the resources nowhere else, so
// `manifest_base` states the fields that need no resource list and the two lists override them here
pub(super) fn build_manifest(
    asset: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
    vt_matches: &[GtpMatch],
) -> ExportManifest {
    let textures = texture_summaries(asset, sources);
    let virtual_textures = virtual_texture_summaries(asset, vt_matches, materials, sources);
    ExportManifest {
        materials: manifest_materials(asset, materials, &textures, &virtual_textures, sources),
        ..manifest_base(asset, sources)
    }
}

// The manifest fields that need no resource list: the asset's identity and where it came from
fn manifest_base(asset: &VisualAsset, sources: &ModSources) -> ExportManifest {
    ExportManifest {
        id: asset.id.clone(),
        name: asset.name.clone(),
        source: source_of(sources, &asset.id),
        path: asset.gr2_path.clone(),
        materials: Vec::new(),
        exported_at_unix: unix_now(),
        maclarian_version: maclarian::VERSION.to_string(),
    }
}

// Seconds since the Unix epoch, or 0 for a clock before it
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// Write `asset.json`; the artifacts are already on disk by then, so a manifest failure is recorded as
// a warning instead of failing the whole export
pub(super) fn write_manifest(manifest: &ExportManifest, out_dir: &Path, output: &mut ExportOutput) {
    let manifest_path = out_dir.join("asset.json");
    match serde_json::to_string_pretty(manifest) {
        Ok(json) => {
            if let Err(err) = fs::write(&manifest_path, json) {
                output.warn("manifestWriteFailed", err.to_string());
            }
        }
        Err(err) => output.warn("manifestSerializeFailed", err.to_string()),
    }
}

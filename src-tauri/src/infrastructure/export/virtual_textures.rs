//! The virtual-texture step: stages the page files of each virtual texture, runs maclarian's
//! extractor over them, moves the resulting layer files into the export directory, and sizes the
//! manifest rows whose page file resolved.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use maclarian::merged::{GtpMatch, VirtualTextureRef, VisualAsset};
use maclarian::virtual_texture::VirtualTextureExtractor;

use super::plan::{ExportPlan, ProgressTracker};
use super::textures::replace_dds_with_png;
use super::{push_warning, record_file, PHASE_VIRTUAL};
use crate::domain::export::{ExportManifest, ExportWarning, ExportedFile};
use crate::domain::naming::{export_layer_name, sanitize_file_name};
use crate::domain::virtual_textures::{
    self, match_for_hash, PageFileSizes, StagedSources, VirtualTextureSummary,
};
use crate::infrastructure::archives::{lock_pool, Archives};

/// The three layers exported from a virtual texture (order matches `VirtualTextureLayer`).
/// Extractor output is `<name>_<layer>.dds`; export file names follow the engine's naming for split
/// virtual textures (`BaseMap` → `Albedo`, see `domain::naming::export_layer_name`)
pub(super) const VT_LAYERS: [&str; 3] = ["BaseMap", "NormalMap", "PhysicalMap"];

/// Page files worth extracting: extracting needs a hash to resolve the `GtpMatch` naming the page
/// file in the archives, and virtual textures are only exported together with the textures
pub(crate) fn vt_targets_of(asset: &VisualAsset, export_textures: bool) -> Vec<&VirtualTextureRef> {
    if !export_textures {
        return Vec::new();
    }
    asset
        .virtual_textures
        .iter()
        .filter(|vt| !vt.gtex_hash.trim().is_empty())
        .collect()
}

/// Extract every virtual texture of the asset. All of them share one staging directory — a GTS is
/// read once no matter how many page files of its tile set pass through — while each get its own
/// extraction output directory, so a failed candidate cannot leave artifacts behind for the next
pub(super) fn export_virtual_textures(
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    vt_matches: &[GtpMatch],
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    if plan.vt_targets.is_empty() {
        return Ok(());
    }

    let vt_dir = plan.vt_dir();
    fs::create_dir_all(&vt_dir)
        .map_err(|e| format!("Failed to create virtual textures directory: {e}"))?;

    let staging = VtStaging::new();
    if let Err(err) = staging.prepare() {
        push_warning(warnings, "vtStagingFailed", err.to_string());
        return Ok(());
    }

    for (seq, vt) in plan.vt_targets.iter().enumerate() {
        progress.item(PHASE_VIRTUAL, Some(vt.name.clone()));
        export_vt_target(pool, vt, vt_matches, seq, &staging, plan, files, warnings)?;
    }

    Ok(())
}

/// Extract one virtual texture in its own staging directory. A failure is recorded as a warning:
/// unlike the mesh, one broken virtual texture does not abort the export
#[allow(clippy::too_many_arguments)]
pub(super) fn export_vt_target(
    pool: &Arc<Mutex<Archives>>,
    vt: &VirtualTextureRef,
    vt_matches: &[GtpMatch],
    seq: usize,
    staging: &VtStaging,
    plan: &ExportPlan<'_>,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    let Some(matched) = match_for_hash(vt_matches, &vt.gtex_hash) else {
        push_warning(warnings, "vtGtpNotFound", vt.gtex_hash.clone());
        return Ok(());
    };

    let stage = staging.stage(seq);
    let mut archive = lock_pool(pool)?;
    if let Err(err) = export_virtual_texture(
        &mut archive,
        matched,
        &vt.name,
        seq,
        plan,
        staging,
        files,
        warnings,
    ) {
        push_warning(warnings, "vtFailed", format!("{}: {err}", vt.name));
    }
    // Released per virtual texture, so a long export keeps interleaving with previews
    drop(archive);
    let _ = fs::remove_dir_all(&stage);

    Ok(())
}

/// Staging area of one export run: all page files share `files` (a GTS is read once no matter how
/// many of them use it) while each get its own output directory `stage_N`. The whole area is
/// removed when the run ends — also when it is aborted halfway
pub(super) struct VtStaging {
    root: PathBuf,
}

impl VtStaging {
    pub(super) fn new() -> Self {
        Self {
            root: std::env::temp_dir().join(format!(
                "bg3_asset_export_{}_{}",
                std::process::id(),
                next_temp_id()
            )),
        }
    }

    /// Files staged for all page files (`virtual_textures::stage_sources` reuses them by name)
    pub(super) fn shared(&self) -> PathBuf {
        self.root.join("files")
    }

    /// Output directory of the page file with this export sequence number
    pub(super) fn stage(&self, seq: usize) -> PathBuf {
        self.root.join(format!("stage_{seq}"))
    }

    pub(super) fn prepare(&self) -> Result<(), String> {
        fs::create_dir_all(self.shared()).map_err(|e| e.to_string())
    }
}

impl Drop for VtStaging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(super) fn next_temp_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Extract a single virtual texture: pull the page file `matched` names (plus the GTS beside it),
/// resolve the three layer DDS files, and write them to the export dir.
#[allow(clippy::too_many_arguments)]
pub(super) fn export_virtual_texture(
    pak: &mut Archives,
    matched: &GtpMatch,
    vt_name: &str,
    seq: usize,
    plan: &ExportPlan<'_>,
    staging: &VtStaging,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    let stage = staging.stage(seq);
    fs::create_dir_all(&stage).map_err(|e| format!("Failed to create staging directory: {e}"))?;

    // Which files the archives hold for this page file is `virtual_textures`' question; the export
    // only consumes the staged paths
    let StagedSources {
        gtp,
        gts_candidates,
    } = virtual_textures::stage_sources(pak, matched, &staging.shared())?;

    extract_with_any_gts(&gtp, gts_candidates, &stage)?;

    let safe_name = sanitize_file_name(vt_name);
    collect_vt_layers(
        &stage,
        &plan.vt_dir(),
        &safe_name,
        plan.convert_to_png,
        files,
        warnings,
    )
}

/// Run the extractor with each GTS candidate until one accepts the page file: GTS naming does not
/// always match the GTP, so the candidates are tried in likelihood order. Trying them is safe
/// because the extractor checks the hash against the GTS metadata itself
pub(super) fn extract_with_any_gts(
    gtp: &Path,
    gts_candidates: Vec<PathBuf>,
    stage: &Path,
) -> Result<(), String> {
    let mut last_err = "no GTS candidate available".to_string();
    for gts_path in gts_candidates {
        match VirtualTextureExtractor::extract_with_gts(gtp, &gts_path, stage) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err.to_string();
                // Clear partial output so the next candidate's artifacts do not mix together
                let _ = fs::remove_dir_all(stage);
                fs::create_dir_all(stage)
                    .map_err(|e| format!("Failed to recreate staging directory: {e}"))?;
            }
        }
    }

    Err(format!("Virtual texture extraction failed: {last_err}"))
}

/// Move the extracted layer files into `vt_dir`, named after the asset instead of the tile set, and
/// convert them when PNG was asked for
pub(super) fn collect_vt_layers(
    stage: &Path,
    vt_dir: &Path,
    safe_name: &str,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    for layer in VT_LAYERS {
        let Some(src) = find_layer_output(stage, layer)? else {
            continue;
        };
        export_vt_layer(
            layer,
            &src,
            vt_dir,
            safe_name,
            convert_to_png,
            files,
            warnings,
        )?;
    }

    Ok(())
}

/// Move one extracted layer into `vt_dir` under its export name and record the artifact
pub(super) fn export_vt_layer(
    layer: &str,
    src: &Path,
    vt_dir: &Path,
    safe_name: &str,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    // The extractor writes `<name>_<layer>.dds` (e.g. `..._basemap.dds`); the export file is named
    // after the layer as the engine names it
    let stem = format!("{safe_name}_{}", export_layer_name(layer));
    // Same name, same place: a re-export overwrites the file the previous one left behind. A rename
    // onto an existing destination fails on Windows, which is what the copy below covers
    let dds_path = vt_dir.join(format!("{stem}.dds"));
    if fs::rename(src, &dds_path).is_err() {
        fs::copy(src, &dds_path).map_err(|e| format!("Failed to move {layer} output: {e}"))?;
    }

    let dds_bytes = fs::read(&dds_path).unwrap_or_default();
    let replaced = convert_to_png
        && replace_dds_with_png(&dds_bytes, vt_dir, &stem, &dds_path, files, warnings, &stem);
    if !replaced {
        record_file(files, &dds_path, "dds", dds_bytes.len());
    }

    Ok(())
}

/// Find the extractor output for one layer; the file name ends with `_<layer>.dds`
pub(super) fn find_layer_output(stage: &Path, layer: &str) -> Result<Option<PathBuf>, String> {
    let suffix = format!("_{}.dds", layer.to_lowercase());
    Ok(fs::read_dir(stage)
        .map_err(|e| format!("Failed to list extraction output: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_lowercase()
                .ends_with(&suffix)
        }))
}

/// Fill in the pixel size of every virtual texture the materials of the manifest carry.
///
/// The size is the bounding box of that page file's own tiles, which is exactly the DDS the extractor
/// wrote next to the manifest and the value the detail view shows for the same virtual texture (see
/// `virtual_textures::page_file_size`), so `asset.json` and the UI agree on it.
///
/// The archives are read here, and a size is decoration: an unavailable pool, a hash that resolved to
/// no page file or a GTS that does not parse all leave the field unset rather than failing an export
/// whose files are already on disk.
pub(super) fn fill_vt_sizes(
    pool: &Arc<Mutex<Archives>>,
    vt_matches: &[GtpMatch],
    manifest: &mut ExportManifest,
) {
    if manifest
        .materials
        .iter()
        .all(|material| material.virtual_textures.is_empty())
    {
        return;
    }

    let Ok(mut archives) = lock_pool(pool) else {
        return;
    };

    // One GTS serves every page file of its tile set, so a shared cache reads each of them once — also
    // when two materials of the asset bind the same virtual texture
    let mut sizes: HashMap<String, PageFileSizes> = HashMap::new();
    for material in &mut manifest.materials {
        for row in &mut material.virtual_textures {
            fill_vt_size(&mut archives, vt_matches, &mut sizes, row);
        }
    }
}

/// Size of the page file one row resolved to, or nothing when it did not resolve
pub(super) fn fill_vt_size(
    archives: &mut Archives,
    vt_matches: &[GtpMatch],
    sizes: &mut HashMap<String, PageFileSizes>,
    row: &mut VirtualTextureSummary,
) {
    let size = match_for_hash(vt_matches, &row.hash)
        .and_then(|matched| virtual_textures::page_file_size(archives, sizes, matched, &row.hash));
    row.set_size(size);
}

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
use super::{ExportContext, ExportOutput, PHASE_VIRTUAL};
use crate::domain::export::ExportManifest;
use crate::domain::naming::{export_layer_name, sanitize_file_name};
use crate::domain::virtual_textures::{self, match_for_hash, PageFileSizes, VirtualTextureSummary};
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
    ctx: &ExportContext<'_>,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    if plan.vt_targets.is_empty() {
        return Ok(());
    }
    let Some(staging) = vt_staging(plan, output)? else {
        return Ok(());
    };
    export_vt_targets(ctx, plan, &staging, output, progress)
}

/// Create the virtual texture output directory and the staging area of the run. `None` when the
/// staging area cannot be made, which is a warning: the export carries on without virtual textures
fn vt_staging(plan: &ExportPlan<'_>, output: &mut ExportOutput) -> Result<Option<VtStaging>, String> {
    fs::create_dir_all(plan.vt_dir())
        .map_err(|e| format!("Failed to create virtual textures directory: {e}"))?;
    let staging = VtStaging::new();
    if let Err(err) = staging.prepare() {
        output.warn("vtStagingFailed", err.to_string());
        return Ok(None);
    }
    Ok(Some(staging))
}

/// Extract every target of the plan, one progress item each
fn export_vt_targets(
    ctx: &ExportContext<'_>,
    plan: &ExportPlan<'_>,
    staging: &VtStaging,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    for (seq, vt) in plan.vt_targets.iter().enumerate() {
        progress.item(PHASE_VIRTUAL, Some(vt.name.clone()));
        export_vt_target(ctx, vt, seq, staging, plan, output)?;
    }
    Ok(())
}

/// One virtual texture of an export run: the target, the page file its hash resolved to, and the
/// sequence number of its staging directory
struct VtJob<'a> {
    vt: &'a VirtualTextureRef,
    matched: &'a GtpMatch,
    seq: usize,
}

impl<'a> VtJob<'a> {
    /// The job of one target; `None` when its hash resolved to no page file
    fn resolve(vt: &'a VirtualTextureRef, vt_matches: &'a [GtpMatch], seq: usize) -> Option<Self> {
        let matched = match_for_hash(vt_matches, &vt.gtex_hash)?;
        Some(Self { vt, matched, seq })
    }
}

/// Extract one virtual texture. A hash that resolved to no page file, and an extraction that fails,
/// are both warnings: unlike the mesh, one broken virtual texture does not abort the export
fn export_vt_target(
    ctx: &ExportContext<'_>,
    vt: &VirtualTextureRef,
    seq: usize,
    staging: &VtStaging,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
) -> Result<(), String> {
    let Some(job) = VtJob::resolve(vt, ctx.vt_matches, seq) else {
        output.warn("vtGtpNotFound", vt.gtex_hash.clone());
        return Ok(());
    };
    extract_vt_target(ctx, &job, staging, plan, output)
}

/// Extract one resolved target in its own staging directory, releasing the archive pool afterward
/// so a long export keeps interleaving with previews. Reaching the archives is not optional —
/// nothing can be written without them — while a failed extraction is only a warning
fn extract_vt_target(
    ctx: &ExportContext<'_>,
    job: &VtJob<'_>,
    staging: &VtStaging,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
) -> Result<(), String> {
    let mut archive = lock_pool(ctx.pool)?;
    if let Err(err) = extract_vt(&mut archive, job, staging, plan, output) {
        output.warn("vtFailed", format!("{}: {err}", job.vt.name));
    }
    let _ = fs::remove_dir_all(staging.stage(job.seq));
    Ok(())
}

/// Extract one-page file into its staging directory and move the layer files into the export dir
fn extract_vt(
    archive: &mut Archives,
    job: &VtJob<'_>,
    staging: &VtStaging,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
) -> Result<(), String> {
    let stage = staging.stage(job.seq);
    fs::create_dir_all(&stage).map_err(|e| format!("Failed to create staging directory: {e}"))?;
    // Which files the archives hold for this page file is `virtual_textures`' question; the export
    // only consumes the staged paths
    let sources = virtual_textures::stage_sources(archive, job.matched, &staging.shared())?;
    extract_with_any_gts(&sources.gtp, &sources.gts_candidates, &stage)?;
    collect_vt_layers(&stage, plan, &sanitize_file_name(&job.vt.name), output)
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

/// Run the extractor with each GTS candidate until one accepts the page file: GTS naming does not
/// always match the GTP, so the candidates are tried in likelihood order. Trying them is safe
/// because the extractor checks the hash against the GTS metadata itself
fn extract_with_any_gts(
    gtp: &Path,
    gts_candidates: &[PathBuf],
    stage: &Path,
) -> Result<(), String> {
    extract_each_gts(gtp, gts_candidates, stage)
        .map_err(|last_err| format!("Virtual texture extraction failed: {last_err}"))
}

/// Try every candidate in order, reporting the last error when none of them accepts the page file
fn extract_each_gts(gtp: &Path, gts_candidates: &[PathBuf], stage: &Path) -> Result<(), String> {
    let mut last_err = "no GTS candidate available".to_string();
    for gts_path in gts_candidates {
        let Some(err) = attempt_gts(gtp, gts_path, stage)? else {
            return Ok(());
        };
        last_err = err;
    }
    Err(last_err)
}

/// Try one candidate: `Ok(None)` when it accepted the page file, `Ok(Some(error))` when it did not,
/// with the partial output cleared so the next candidate's artifacts do not mix together
fn attempt_gts(gtp: &Path, gts_path: &Path, stage: &Path) -> Result<Option<String>, String> {
    match VirtualTextureExtractor::extract_with_gts(gtp, gts_path, stage) {
        Ok(()) => Ok(None),
        Err(err) => {
            reset_stage(stage)?;
            Ok(Some(err.to_string()))
        }
    }
}

/// Clear the staging directory so the next candidate starts from nothing, recreating it because the
/// extractor expects it to exist
fn reset_stage(stage: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(stage);
    fs::create_dir_all(stage).map_err(|e| format!("Failed to recreate staging directory: {e}"))
}

/// Move the extracted layer files into the export's virtual texture directory, named after the asset
/// instead of the tile set, and convert them when PNG was asked for
fn collect_vt_layers(
    stage: &Path,
    plan: &ExportPlan<'_>,
    safe_name: &str,
    output: &mut ExportOutput,
) -> Result<(), String> {
    for layer in VT_LAYERS {
        let Some(src) = find_layer_output(stage, layer)? else {
            continue;
        };
        export_vt_layer(layer, &src, plan, safe_name, output)?;
    }
    Ok(())
}

/// Move one extracted layer into the virtual texture directory under its export name and record the
/// artifact
fn export_vt_layer(
    layer: &str,
    src: &Path,
    plan: &ExportPlan<'_>,
    safe_name: &str,
    output: &mut ExportOutput,
) -> Result<(), String> {
    // The extractor writes `<name>_<layer>.dds` (e.g. `..._basemap.dds`); the export file is named
    // after the layer as the engine names it
    let stem = format!("{safe_name}_{}", export_layer_name(layer));
    let vt_dir = plan.vt_dir();
    let dds_path = vt_dir.join(format!("{stem}.dds"));
    move_layer(src, &dds_path, layer)?;
    record_layer(&vt_dir, &dds_path, &stem, plan.convert_to_png, output);
    Ok(())
}

/// Move the extractor's output into place. A rename onto an existing destination fails on Windows,
/// which is what the copy covers: a re-export overwrites the file the previous one left behind
fn move_layer(src: &Path, dds_path: &Path, layer: &str) -> Result<(), String> {
    if fs::rename(src, dds_path).is_ok() {
        return Ok(());
    }
    fs::copy(src, dds_path).map_err(|e| format!("Failed to move {layer} output: {e}"))?;
    Ok(())
}

/// Record the layer's DDS, replaced by its PNG form when conversion was asked for and worked
fn record_layer(
    vt_dir: &Path,
    dds_path: &Path,
    stem: &str,
    convert_to_png: bool,
    output: &mut ExportOutput,
) {
    let dds_bytes = fs::read(dds_path).unwrap_or_default();
    let png_path = vt_dir.join(format!("{stem}.png"));
    let replaced =
        convert_to_png && replace_dds_with_png(&dds_bytes, &png_path, dds_path, output, stem);
    if !replaced {
        output.record(dds_path, "dds", dds_bytes.len());
    }
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
    if !has_vt_rows(manifest) {
        return;
    }
    let Ok(mut archives) = lock_pool(pool) else {
        return;
    };
    fill_sizes(&mut archives, vt_matches, manifest);
}

/// Whether the manifest carries any virtual texture row at all; nothing is read from the archives
/// when it does not
fn has_vt_rows(manifest: &ExportManifest) -> bool {
    manifest
        .materials
        .iter()
        .any(|material| !material.virtual_textures.is_empty())
}

/// Size every virtual texture row of the manifest, over one shared GTS cache: one GTS serves every
/// page file of its tile set, so each is read once — also when two materials of the asset bind the
/// same virtual texture
fn fill_sizes(archives: &mut Archives, vt_matches: &[GtpMatch], manifest: &mut ExportManifest) {
    let mut sizes: HashMap<String, PageFileSizes> = HashMap::new();
    for material in &mut manifest.materials {
        for row in &mut material.virtual_textures {
            fill_vt_size(archives, vt_matches, &mut sizes, row);
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
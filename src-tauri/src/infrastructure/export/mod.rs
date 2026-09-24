//! Asset export pipeline: exports a full visual asset (GR2 mesh + textures + virtual textures +
//! metadata) into a target directory.
//!
//! All format conversions reuse maclarian instead of reimplementing anything:
//! - `convert_gr2_bytes_to_glb`: GR2 → GLB (mesh only, no embedded textures)
//! - `dds_bytes_to_png_bytes`: DDS → PNG (direct in-memory conversion, no intermediate files)
//! - `VirtualTextureExtractor`: GTP + GTS → three layer DDS files (BaseMap / NormalMap / PhysicalMap)
//!
//! Reaching into the archives is not this module's job: `archives` beside it owns the PAK read pool
//! and `crate::domain::virtual_textures` stages the page files the extractor consumes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use maclarian::merged::{GtpMatch, VisualAsset};

use crate::domain::export::{
    ExportOptions, ExportProgress, ExportResult, ExportWarning, ExportedFile,
};
use crate::domain::material::MaterialInfo;
use crate::domain::source::ModSources;
use crate::infrastructure::archives::Archives;

use self::manifest::{build_manifest, write_manifest};
use self::mesh::export_mesh;
use self::plan::{plan_export, ProgressTracker};
use self::textures::export_textures;
use self::virtual_textures::{export_virtual_textures, fill_vt_sizes};

mod manifest;
mod mesh;
mod plan;
mod textures;
mod virtual_textures;

pub use textures::try_write_png_and_record;

/// Progress phases (the frontend uses these to look up i18n copy)
pub(crate) const PHASE_PREPARE: &str = "prepare";
pub(crate) const PHASE_MODEL: &str = "model";
/// Raw GR2 copy (no conversion happening, so the frontend shows different copy)
pub(crate) const PHASE_MODEL_RAW: &str = "modelRaw";
pub(crate) const PHASE_TEXTURES: &str = "textures";
pub(crate) const PHASE_VIRTUAL: &str = "virtualTextures";
pub(crate) const PHASE_MANIFEST: &str = "manifest";
pub(crate) const PHASE_DONE: &str = "done";

/// Append a structured warning (code is localized by the frontend, detail keeps the raw message)
pub(crate) fn push_warning(
    warnings: &mut Vec<ExportWarning>,
    code: &str,
    detail: impl Into<String>,
) {
    warnings.push(ExportWarning {
        code: code.to_string(),
        detail: detail.into(),
    });
}

/// Record one written artifact in the export result
pub(crate) fn record_file(
    files: &mut Vec<ExportedFile>,
    path: &Path,
    kind: &str,
    size_bytes: usize,
) {
    files.push(ExportedFile {
        path: path.display().to_string(),
        kind: kind.to_string(),
        size_bytes,
    });
}

/// What an export reads from: the asset being exported and the caches describing it, grouped so the
/// entry point takes one input instead of a parameter list that only grows with each new source.
pub struct ExportContext<'a> {
    /// The asset to export, already resolved by GUID
    pub asset: &'a VisualAsset,
    /// Material cache entries of this asset's materials, taken by the command that holds the cache
    /// (see `domain::material::materials_of`). The manifest names the material rows with them and
    /// reads every resource's bindings from them, so they are the one piece of state that cannot be
    /// missing.
    pub materials: &'a HashMap<String, MaterialInfo>,
    /// Labels each manifest row with the mod providing it; the game's own rows stay unlabeled
    pub sources: &'a ModSources,
    /// Page files resolved for this asset's virtual textures (see
    /// `application::state::AppState::vt_matches`)
    pub vt_matches: &'a [GtpMatch],
    /// The archives every read above goes through
    pub pool: &'a Arc<Mutex<Archives>>,
}

/// Export a single visual asset. The GLB is the core artifact — its failure aborts the whole
/// export, while a single texture / virtual texture failure only records a warning.
pub fn run_export(
    ctx: &ExportContext<'_>,
    dest_root: &Path,
    options: &ExportOptions,
    on_progress: &dyn Fn(ExportProgress),
) -> Result<ExportResult, String> {
    let ExportContext {
        asset,
        materials,
        sources,
        vt_matches,
        pool,
    } = *ctx;

    let plan = plan_export(asset, dest_root, options)?;

    let mut files: Vec<ExportedFile> = Vec::new();
    let mut warnings: Vec<ExportWarning> = Vec::new();
    let mut progress = ProgressTracker::new(plan.progress_total, on_progress);
    progress.phase(PHASE_PREPARE, 0.0);

    // 1. Mesh: raw GR2 straight out of the PAK, or a plain GR2 → GLB conversion
    export_mesh(asset, pool, &plan, &mut files, &mut progress)?;

    // 2. Textures: pull DDS from PAKs, optionally converting to PNG
    export_textures(asset, pool, &plan, &mut files, &mut warnings, &mut progress)?;

    // 3. Virtual textures: GTP + GTS → three layer DDS files. Every hash was resolved to a
    // `GtpMatch` up front, which carries both the page file and the archive holding it
    export_virtual_textures(
        pool,
        &plan,
        vt_matches,
        &mut files,
        &mut warnings,
        &mut progress,
    )?;

    // 4. Metadata manifest (always written, but never listed among the exported files)
    progress.item(PHASE_MANIFEST, None);
    let mut manifest = build_manifest(asset, materials, sources, vt_matches);
    // Completed before to write: the sizes come out of the archives, which the manifest alone has
    // no access to
    fill_vt_sizes(pool, vt_matches, &mut manifest);
    write_manifest(&manifest, &plan.out_dir, &mut warnings);

    progress.phase(PHASE_DONE, 1.0);
    Ok(ExportResult {
        output_dir: plan.out_dir.display().to_string(),
        files,
        warnings,
    })
}

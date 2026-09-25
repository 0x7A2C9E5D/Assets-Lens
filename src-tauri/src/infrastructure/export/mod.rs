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
use self::plan::{plan_export, ExportPlan, ProgressTracker};
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

/// What one export run collected: the artifacts it wrote and the warnings they raised.
///
/// Every step of the pipeline — mesh, textures, virtual textures, manifest — contributes to the same
/// two lists, so they travel as one output each step appends to instead of two parameters threaded
/// through every call.
#[derive(Default)]
pub struct ExportOutput {
    files: Vec<ExportedFile>,
    warnings: Vec<ExportWarning>,
}

impl ExportOutput {
    /// Record one written artifact
    pub(crate) fn record(&mut self, path: &Path, kind: &str, size_bytes: usize) {
        self.files.push(ExportedFile {
            path: path.display().to_string(),
            kind: kind.to_string(),
            size_bytes,
        });
    }

    /// Append a structured warning (code is localized by the frontend, detail keeps the raw message)
    pub(crate) fn warn(&mut self, code: &str, detail: impl Into<String>) {
        self.warnings.push(ExportWarning {
            code: code.to_string(),
            detail: detail.into(),
        });
    }

    /// The artifacts and warnings, in the order the pipeline produced them
    pub(crate) fn into_parts(self) -> (Vec<ExportedFile>, Vec<ExportWarning>) {
        (self.files, self.warnings)
    }
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
    let plan = plan_export(ctx.asset, dest_root, options)?;
    let mut output = ExportOutput::default();
    let mut progress = ProgressTracker::new(plan.progress_total, on_progress);
    progress.phase(PHASE_PREPARE, 0.0);

    export_artifacts(ctx, &plan, &mut output, &mut progress)?;
    export_manifest(ctx, &plan, &mut output, &mut progress);

    progress.phase(PHASE_DONE, 1.0);
    Ok(export_result(&plan, output))
}

/// Write the artifacts of one export, in pipeline order: the mesh (raw GR2 or a GR2 → GLB
/// conversion), then the textures, then the virtual textures.
///
/// The mesh is the core artifact, so its failure aborts the whole export; a single texture or
/// virtual texture that fails only records a warning and the rest carries on.
fn export_artifacts(
    ctx: &ExportContext<'_>,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    export_mesh(ctx.asset, ctx.pool, plan, output, progress)?;
    export_textures(ctx.asset, ctx.pool, plan, output, progress)?;
    export_virtual_textures(ctx, plan, output, progress)
}

/// Write the metadata manifest (`asset.json`). It is always written and never listed among the
/// exported files.
fn export_manifest(
    ctx: &ExportContext<'_>,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) {
    progress.item(PHASE_MANIFEST, None);
    let mut manifest = build_manifest(ctx.asset, ctx.materials, ctx.sources, ctx.vt_matches);
    // Completed before writing: the sizes come out of the archives, which the manifest alone has no
    // access to
    fill_vt_sizes(ctx.pool, ctx.vt_matches, &mut manifest);
    write_manifest(&manifest, &plan.out_dir, output);
}

/// Where the export wrote and what it produced
fn export_result(plan: &ExportPlan<'_>, output: ExportOutput) -> ExportResult {
    let (files, warnings) = output.into_parts();
    ExportResult {
        output_dir: plan.out_dir.display().to_string(),
        files,
        warnings,
    }
}

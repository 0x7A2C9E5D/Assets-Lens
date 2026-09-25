//! The mesh step: reads the asset's GR2 out of the archives and writes it, converted to GLB unless
//! the raw GR2 was asked for.

use std::fs;
use std::sync::{Arc, Mutex};

use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::VisualAsset;

use super::plan::{ExportPlan, ProgressTracker};
use super::{ExportOutput, PHASE_MODEL, PHASE_MODEL_RAW};
use crate::domain::export::MeshFormat;
use crate::infrastructure::archives::{lock_pool, Archives, Pak};

// Read the mesh and write it into the export directory; unlike a texture the mesh is not optional,
// so any failure here aborts the whole export instead of recording a warning
pub(super) fn export_mesh(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    report_mesh_start(asset, plan, progress);
    let mesh_bytes = read_mesh_bytes(asset, pool, plan.mesh_format)?;
    write_mesh(&mesh_bytes, plan, output)
}

// Count the mesh as the current item, under the phase its format belongs to
fn report_mesh_start(
    asset: &VisualAsset,
    plan: &ExportPlan<'_>,
    progress: &mut ProgressTracker<'_>,
) {
    let phase = if plan.mesh_format.is_glb() {
        PHASE_MODEL
    } else {
        PHASE_MODEL_RAW
    };
    progress.item(phase, Some(asset.gr2_path.clone()));
}

// Write the mesh bytes into the export directory and record the artifact
fn write_mesh(
    bytes: &[u8],
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
) -> Result<(), String> {
    let ext = plan.mesh_format.extension();
    let path = plan.out_dir.join(format!("{}.{ext}", plan.dir_name));
    fs::write(&path, bytes).map_err(|e| format!("Failed to write mesh ({ext}): {e}"))?;
    output.record(&path, ext, bytes.len());
    Ok(())
}

// Pull the mesh bytes out of the archives, converted to GLB unless the raw GR2 was asked for
pub(super) fn read_mesh_bytes(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    mesh_format: MeshFormat,
) -> Result<Vec<u8>, String> {
    // The mesh is read from `Models.pak` and nowhere else; a GR2 the game ships in another archive
    // reports that rather than being searched for
    let gr2_bytes = lock_pool(pool)?
        .read_from(Pak::Models, &asset.gr2_path)
        .map_err(|err| format!("Mesh data unavailable: {err}"))?;

    match mesh_format {
        // Raw GR2: the archive bytes are the artifact, nothing to convert
        MeshFormat::Gr2 => Ok(gr2_bytes),
        MeshFormat::Glb => convert_gr2_bytes_to_glb(&gr2_bytes)
            .map_err(|e| format!("Failed to convert mesh to GLB: {e}")),
    }
}

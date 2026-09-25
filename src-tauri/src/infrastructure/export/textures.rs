//! The regular-texture step: pulls each DDS out of the archives, optionally converts it to PNG, and
//! records (or warns about) every artifact along with the shared PNG-writing helpers.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use maclarian::converter::dds_bytes_to_png_bytes;
use maclarian::merged::{TextureRef, VisualAsset};

use super::plan::{ExportPlan, ProgressTracker};
use super::{ExportOutput, PHASE_TEXTURES};
use crate::domain::naming::sanitize_file_name;
use crate::infrastructure::archives::{lock_pool, Archives, Pak};

/// Write a PNG; on success remove the intermediate DDS and record an export entry, on failure push
/// a `pngWriteFailed` warning. Returns `true` when the PNG has been written (callers then skip the
/// DDS fallback entry).
pub fn try_write_png_and_record(
    png: &[u8],
    png_path: &Path,
    dds_path: &Path,
    output: &mut ExportOutput,
    source_label: &str,
) -> bool {
    if let Err(err) = fs::write(png_path, png) {
        output.warn("pngWriteFailed", format!("{source_label}: {err}"));
        return false;
    }
    let _ = fs::remove_file(dds_path);
    output.record(png_path, "png", png.len());
    true
}

/// Write the PNG form of the DDS at `dds_path` and drop the DDS once the PNG is in place. Returns
/// `true` when the PNG replaced the DDS (callers then skip the DDS entry), `false` when the DDS has
/// to stay: either the conversion or the write failed, each reporting its own warning
pub(crate) fn replace_dds_with_png(
    dds_bytes: &[u8],
    png_path: &Path,
    dds_path: &Path,
    output: &mut ExportOutput,
    source_label: &str,
) -> bool {
    match dds_bytes_to_png_bytes(dds_bytes) {
        Ok(png) => try_write_png_and_record(&png, png_path, dds_path, output, source_label),
        Err(err) => {
            output.warn("pngConvertFailed", format!("{source_label}: {err}"));
            false
        }
    }
}

/// Pull every regular texture of the asset out of the archives, optionally converting it to PNG.
/// A missing texture is a warning: the rest of the export carries on
pub(super) fn export_textures(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    // Regular textures are optional: nothing to do when only the mesh was asked for, or when the
    // asset has no material textures at all
    if !plan.export_textures || asset.textures.is_empty() {
        return Ok(());
    }
    let tex_dir = textures_dir(plan)?;
    export_texture_list(asset, pool, plan, &tex_dir, output, progress)
}

/// Create the directory the regular textures go into
fn textures_dir(plan: &ExportPlan<'_>) -> Result<PathBuf, String> {
    let dir = plan.out_dir.join("textures");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create textures directory: {e}"))?;
    Ok(dir)
}

/// Export every texture of the asset into `tex_dir`, counting one progress item each
fn export_texture_list(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    tex_dir: &Path,
    output: &mut ExportOutput,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    for tex in &asset.textures {
        progress.item(PHASE_TEXTURES, Some(tex.dds_path.clone()));
        export_texture(tex, pool, tex_dir, plan.convert_to_png, output)?;
    }
    Ok(())
}

/// Read one texture out of the archives and write it into `tex_dir`
pub(super) fn export_texture(
    tex: &TextureRef,
    pool: &Arc<Mutex<Archives>>,
    tex_dir: &Path,
    convert_to_png: bool,
    output: &mut ExportOutput,
) -> Result<(), String> {
    // Locked per file only: decompressing one texture is quick, and it leaves the pool available to
    // other commands (a preview) while the export runs
    match lock_pool(pool)?.read_from(Pak::Textures, &tex.dds_path) {
        Ok(dds) => write_texture(tex, &dds, tex_dir, convert_to_png, output),
        Err(err) => output.warn("textureUnavailable", err),
    }

    Ok(())
}

/// Write one texture into `tex_dir`: its DDS, or only the PNG when converting
pub(super) fn write_texture(
    tex: &TextureRef,
    dds: &[u8],
    tex_dir: &Path,
    convert_to_png: bool,
    output: &mut ExportOutput,
) {
    let stem = texture_stem(tex);
    // Same name, same place: a re-export overwrites the file the previous one left behind
    let dds_path = tex_dir.join(format!("{stem}.dds"));
    if !write_dds(dds, &dds_path, &tex.dds_path, output) {
        return;
    }
    let png_path = tex_dir.join(format!("{stem}.png"));
    keep_dds_or_png(dds, &png_path, &dds_path, convert_to_png, &tex.dds_path, output);
}

/// Write the DDS; a failure is a warning rather than the end of the export. `true` when written
fn write_dds(dds: &[u8], dds_path: &Path, source_label: &str, output: &mut ExportOutput) -> bool {
    if let Err(err) = fs::write(dds_path, dds) {
        output.warn("textureWriteFailed", format!("{source_label}: {err}"));
        return false;
    }
    true
}

/// Keep the DDS, or replace it with its PNG form when conversion was asked for and worked
fn keep_dds_or_png(
    dds: &[u8],
    png_path: &Path,
    dds_path: &Path,
    convert_to_png: bool,
    source_label: &str,
    output: &mut ExportOutput,
) {
    let replaced =
        convert_to_png && replace_dds_with_png(dds, png_path, dds_path, output, source_label);
    if !replaced {
        output.record(dds_path, "dds", dds.len());
    }
}

/// File stem of a texture: named after the actual DDS resource in the archive (e.g. `Body_BM`),
/// never after the material parameter slot (e.g. `ColorTexture`) or the bank display name
pub(super) fn texture_stem(tex: &TextureRef) -> String {
    sanitize_file_name(
        Path::new(&tex.dds_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| tex.parameter_name.as_deref().unwrap_or(&tex.name)),
    )
}

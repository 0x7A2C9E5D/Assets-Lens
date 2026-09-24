//! The regular-texture step: pulls each DDS out of the archives, optionally converts it to PNG, and
//! records (or warns about) every artifact along with the shared PNG-writing helpers.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use maclarian::converter::dds_bytes_to_png_bytes;
use maclarian::merged::{TextureRef, VisualAsset};

use super::plan::{ExportPlan, ProgressTracker};
use super::{push_warning, record_file, PHASE_TEXTURES};
use crate::domain::export::{ExportWarning, ExportedFile};
use crate::domain::naming::sanitize_file_name;
use crate::infrastructure::archives::{lock_pool, Archives, Pak};

/// Write a PNG; on success remove the intermediate DDS and record an export entry, on failure push
/// a `pngWriteFailed` warning. Returns `true` when the PNG has been written (callers then skip the
/// DDS fallback entry).
pub fn try_write_png_and_record(
    png: &[u8],
    png_path: &Path,
    dds_path: &Path,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    source_label: &str,
) -> bool {
    if let Err(err) = fs::write(png_path, png) {
        push_warning(warnings, "pngWriteFailed", format!("{source_label}: {err}"));
        false
    } else {
        let _ = fs::remove_file(dds_path);
        record_file(files, png_path, "png", png.len());
        true
    }
}

/// Write the PNG form of `dds_path` and drop the DDS once the PNG is in place. Returns `true` when
/// the PNG replaced the DDS (callers then skip the DDS entry), `false` when the DDS has to stay:
/// either the conversion or to write failed, each reporting its own warning
pub(crate) fn replace_dds_with_png(
    dds_bytes: &[u8],
    dir: &Path,
    stem: &str,
    dds_path: &Path,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    source_label: &str,
) -> bool {
    match dds_bytes_to_png_bytes(dds_bytes) {
        Ok(png) => {
            let png_path = dir.join(format!("{stem}.png"));
            try_write_png_and_record(&png, &png_path, dds_path, files, warnings, source_label)
        }
        Err(err) => {
            push_warning(
                warnings,
                "pngConvertFailed",
                format!("{source_label}: {err}"),
            );
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
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    // Regular textures are optional: nothing to do when only the mesh was asked for, or when the
    // asset has no material textures at all
    if !plan.export_textures || asset.textures.is_empty() {
        return Ok(());
    }

    let tex_dir = plan.out_dir.join("textures");
    fs::create_dir_all(&tex_dir)
        .map_err(|e| format!("Failed to create textures directory: {e}"))?;

    for tex in &asset.textures {
        progress.item(PHASE_TEXTURES, Some(tex.dds_path.clone()));
        export_texture(tex, pool, &tex_dir, plan.convert_to_png, files, warnings)?;
    }

    Ok(())
}

/// Read one texture out of the archives and write it into `tex_dir`
pub(super) fn export_texture(
    tex: &TextureRef,
    pool: &Arc<Mutex<Archives>>,
    tex_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    // Locked per file only: decompressing one texture is quick, and it leaves the pool available to
    // other commands (a preview) while the export runs
    match lock_pool(pool)?.read_from(Pak::Textures, &tex.dds_path) {
        Ok(dds) => write_texture(tex, &dds, tex_dir, convert_to_png, files, warnings),
        Err(err) => push_warning(warnings, "textureUnavailable", err),
    }

    Ok(())
}

/// Write one texture into `tex_dir`: its DDS, or only the PNG when converting
pub(super) fn write_texture(
    tex: &TextureRef,
    dds: &[u8],
    tex_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) {
    let stem = texture_stem(tex);
    // Same name, same place: a re-export overwrites the file the previous one left behind
    let dds_path = tex_dir.join(format!("{stem}.dds"));
    if let Err(err) = fs::write(&dds_path, dds) {
        push_warning(
            warnings,
            "textureWriteFailed",
            format!("{}: {err}", tex.dds_path),
        );
        return;
    }

    let replaced = convert_to_png
        && replace_dds_with_png(
            dds,
            tex_dir,
            &stem,
            &dds_path,
            files,
            warnings,
            &tex.dds_path,
        );
    if !replaced {
        record_file(files, &dds_path, "dds", dds.len());
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

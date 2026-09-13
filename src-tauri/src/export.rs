//! Asset export pipeline: exports a full visual asset (GR2 mesh + textures + virtual textures +
//! metadata) into a target directory.
//!
//! All format conversions reuse maclarian instead of reimplementing anything:
//! - `convert_gr2_bytes_to_glb`: GR2 → GLB (mesh only, no embedded textures)
//! - `dds_bytes_to_png_bytes`: DDS → PNG (direct in-memory conversion, no intermediate files)
//! - `VirtualTextureExtractor`: GTP + GTS → three layer DDS files (BaseMap / NormalMap / PhysicalMap)
//!
//! Reaching into the archives is not this module's job: `crate::archives` owns the PAK read pool and
//! `crate::virtual_textures` stages the page files the extractor consumes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use maclarian::converter::dds_bytes_to_png_bytes;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use maclarian::virtual_texture::VirtualTextureExtractor;

use crate::archives::{lock_pool, Archives};
use crate::models::{
    match_for_hash,
    ExportManifest, ExportOptions, ExportProgress, ExportResult, ExportWarning, ExportedFile,
    MeshFormat, TextureSummary, VirtualTextureSummary,
};
use crate::virtual_textures::{self, StagedSources};

/// Progress phases (the frontend uses these to look up i18n copy)
const PHASE_PREPARE: &str = "prepare";
const PHASE_MODEL: &str = "model";
/// Raw GR2 copy (no conversion happening, so the frontend shows different copy)
const PHASE_MODEL_RAW: &str = "modelRaw";
const PHASE_TEXTURES: &str = "textures";
const PHASE_VIRTUAL: &str = "virtualTextures";
const PHASE_MANIFEST: &str = "manifest";
const PHASE_DONE: &str = "done";

/// The three layers exported from a virtual texture (order matches `VirtualTextureLayer`).
/// Extractor output is `<name>_<layer>.dds`; export file names drop the trailing `Map`
/// (`BaseMap` → `Base`, see `export_virtual_texture`)
const VT_LAYERS: [&str; 3] = ["BaseMap", "NormalMap", "PhysicalMap"];

/// Append a structured warning (code is localized by the frontend, detail keeps the raw message)
fn push_warning(warnings: &mut Vec<ExportWarning>, code: &str, detail: impl Into<String>) {
    warnings.push(ExportWarning {
        code: code.to_string(),
        detail: detail.into(),
    });
}

/// Sanitize a file/directory name: strip Windows-forbidden and control characters, cap the length
fn sanitize_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "asset".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// Append a sequence number when the target file already exists, so same-named files never
/// overwrite each other
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{ext}"));
    let mut seq = 2usize;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{seq}.{ext}"));
        seq += 1;
    }
    candidate
}

/// Write a PNG; on success remove the intermediate DDS and record an export entry, on failure push
/// a `pngWriteFailed` warning. Returns `true` when the PNG has been written (callers then
/// `continue` to skip the DDS fallback entry).
fn try_write_png_and_record(
    png: &[u8],
    png_path: &Path,
    dds_path: &Path,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    source_label: &str,
) -> bool {
    if let Err(err) = fs::write(png_path, png) {
        push_warning(
            warnings,
            "pngWriteFailed",
            format!("{source_label}: {err}"),
        );
        false
    } else {
        let _ = fs::remove_file(dds_path);
        files.push(ExportedFile {
            path: png_path.display().to_string(),
            kind: "png".to_string(),
            size_bytes: png.len(),
        });
        true
    }
}

/// Tracks export progress: counts finished items and reports the running percentage, so no step of
/// the pipeline has to thread a `done` counter around
struct ProgressTracker<'a> {
    total: usize,
    done: usize,
    on_progress: &'a dyn Fn(ExportProgress),
}

impl<'a> ProgressTracker<'a> {
    fn new(total: usize, on_progress: &'a dyn Fn(ExportProgress)) -> Self {
        Self {
            total,
            done: 0,
            on_progress,
        }
    }

    /// Count one finished item and report it
    fn item(&mut self, phase: &str, file: Option<String>) {
        self.done += 1;
        (self.on_progress)(ExportProgress {
            phase: phase.to_string(),
            current_file: file,
            percent: if self.total == 0 {
                1.0
            } else {
                self.done as f32 / self.total as f32
            },
        });
    }

    /// Report a phase boundary without counting an item (`prepare` / `done`); those carry a fixed
    /// percentage because they wrap the counted items instead of being one of them
    fn phase(&self, phase: &str, percent: f32) {
        (self.on_progress)(ExportProgress {
            phase: phase.to_string(),
            current_file: None,
            percent,
        });
    }
}

/// Export a single visual asset. The GLB is the core artifact — its failure aborts the whole
/// export, while a single texture / virtual texture failure only records a warning.
pub fn run_export(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    dest_root: &Path,
    options: &ExportOptions,
    vt_matches: &[GtpMatch],
    on_progress: &dyn Fn(ExportProgress),
) -> Result<ExportResult, String> {
    let mut files: Vec<ExportedFile> = Vec::new();
    let mut warnings: Vec<ExportWarning> = Vec::new();

    let dir_name = sanitize_file_name(&asset.name);
    let out_dir = dest_root.join(&dir_name);
    fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Failed to create export directory: {e}"))?;

    // Textures are always exported separately (regular + virtual); the format is a single switch
    let extract_textures = options.texture_format.is_export();
    let convert_to_png = options.texture_format.is_png();

    let vt_targets: Vec<&VirtualTextureRef> = if extract_textures {
        asset
            .virtual_textures
            .iter()
            .filter(|vt| !vt.gtex_hash.trim().is_empty())
            .collect()
    } else {
        Vec::new()
    };

    let texture_total = if extract_textures {
        asset.textures.len()
    } else {
        0
    };
    // 1 mesh + 1 manifest, plus texture files and virtual textures
    let total = 2 + texture_total + vt_targets.len();
    let mut progress = ProgressTracker::new(total, on_progress);
    progress.phase(PHASE_PREPARE, 0.0);

    // ---- 1. Mesh: raw GR2 straight out of the PAK, or a plain GR2 → GLB conversion ----
    let mesh_format = options.mesh_format;
    files.push(export_mesh(
        asset,
        pool,
        &out_dir,
        &dir_name,
        mesh_format,
        &mut progress,
    )?);

    // ---- 2. Textures: pull DDS from PAKs, optionally converting to PNG ----
    if extract_textures {
        export_textures(
            asset,
            pool,
            &out_dir,
            convert_to_png,
            &mut files,
            &mut warnings,
            &mut progress,
        )?;
    }

    // ---- 3. Virtual textures: GTP + GTS → three layer DDS files ----
    // Every hash was resolved to a `GtpMatch` up front, which is where both the page file and the
    // archive holding it come from.
    if !vt_targets.is_empty() {
        export_virtual_textures(
            pool,
            &vt_targets,
            vt_matches,
            &out_dir,
            convert_to_png,
            &mut files,
            &mut warnings,
            &mut progress,
        )?;
    }

    // ---- 4. Metadata manifest (always written, but never listed among the exported files) ----
    progress.item(PHASE_MANIFEST, None);
    let manifest = build_manifest(asset, vt_matches, mesh_format, &files);
    write_manifest(&manifest, &out_dir, &mut warnings);

    progress.phase(PHASE_DONE, 1.0);

    Ok(ExportResult {
        output_dir: out_dir.display().to_string(),
        files,
        warnings,
    })
}

/// Read the mesh and write it into `out_dir`. Unlike a texture, the mesh is not optional: any
/// failure here aborts the whole export instead of recording a warning
fn export_mesh(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    out_dir: &Path,
    dir_name: &str,
    mesh_format: MeshFormat,
    progress: &mut ProgressTracker<'_>,
) -> Result<ExportedFile, String> {
    progress.item(
        if mesh_format.is_glb() {
            PHASE_MODEL
        } else {
            PHASE_MODEL_RAW
        },
        Some(asset.gr2_path.clone()),
    );

    let gr2_bytes = lock_pool(pool)?
        .read(&asset.gr2_path, Some("Models.pak"))
        .map_err(|err| format!("Mesh data unavailable: {err}"))?;

    let mesh_bytes = match mesh_format {
        // Raw GR2: the archive bytes are the artifact, nothing to convert
        MeshFormat::Gr2 => gr2_bytes,
        MeshFormat::Glb => convert_gr2_bytes_to_glb(&gr2_bytes)
            .map_err(|e| format!("Failed to convert mesh to GLB: {e}"))?,
    };

    let mesh_ext = mesh_format.extension();
    let mesh_path = out_dir.join(format!("{dir_name}.{mesh_ext}"));
    fs::write(&mesh_path, &mesh_bytes)
        .map_err(|e| format!("Failed to write mesh ({mesh_ext}): {e}"))?;

    Ok(ExportedFile {
        path: mesh_path.display().to_string(),
        kind: mesh_ext.to_string(),
        size_bytes: mesh_bytes.len(),
    })
}

/// Pull every regular texture of the asset out of the archives, optionally converting it to PNG.
/// A missing texture is a warning: the rest of the export carries on
fn export_textures(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    out_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    if asset.textures.is_empty() {
        return Ok(());
    }

    let tex_dir = out_dir.join("textures");
    fs::create_dir_all(&tex_dir)
        .map_err(|e| format!("Failed to create textures directory: {e}"))?;

    for tex in &asset.textures {
        progress.item(PHASE_TEXTURES, Some(tex.dds_path.clone()));

        // Locked per file only: decompressing one texture is quick, and it leaves the pool
        // available to other commands (a preview) while the export runs
        match lock_pool(pool)?.read(&tex.dds_path, Some(tex.source_pak.as_str())) {
            Ok(dds) => write_texture(tex, &dds, &tex_dir, convert_to_png, files, warnings),
            Err(err) => push_warning(warnings, "textureUnavailable", err),
        }
    }

    Ok(())
}

/// Write one texture into `tex_dir`: its DDS, or only the PNG when converting
fn write_texture(
    tex: &TextureRef,
    dds: &[u8],
    tex_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) {
    // Name files after the actual DDS resource in the archive (e.g. `Body_BM`),
    // never the material parameter slot (e.g. `ColorTexture`) or bank display name
    let stem = sanitize_file_name(
        Path::new(&tex.dds_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| tex.parameter_name.as_deref().unwrap_or(&tex.name)),
    );
    let dds_path = unique_path(tex_dir, &stem, "dds");
    if let Err(err) = fs::write(&dds_path, dds) {
        push_warning(
            warnings,
            "textureWriteFailed",
            format!("{}: {err}", tex.dds_path),
        );
        return;
    }

    if convert_to_png {
        match dds_bytes_to_png_bytes(dds) {
            Ok(png) => {
                let png_path = unique_path(tex_dir, &stem, "png");
                if try_write_png_and_record(
                    &png,
                    &png_path,
                    &dds_path,
                    files,
                    warnings,
                    &tex.dds_path,
                ) {
                    return;
                }
            }
            Err(err) => push_warning(
                warnings,
                "pngConvertFailed",
                format!("{}: {err}", tex.dds_path),
            ),
        }
    }

    files.push(ExportedFile {
        path: dds_path.display().to_string(),
        kind: "dds".to_string(),
        size_bytes: dds.len(),
    });
}

/// Extract every virtual texture of the asset. All of them share one staging directory — a GTS is
/// read once no matter how many page files of its tile set pass through — while each get its own
/// extraction output directory, so a failed candidate cannot leave artifacts behind for the next
#[allow(clippy::too_many_arguments)]
fn export_virtual_textures(
    pool: &Arc<Mutex<Archives>>,
    vt_targets: &[&VirtualTextureRef],
    vt_matches: &[GtpMatch],
    out_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), String> {
    let vt_dir = out_dir.join("virtual_textures");
    fs::create_dir_all(&vt_dir)
        .map_err(|e| format!("Failed to create virtual textures directory: {e}"))?;

    let temp_root = std::env::temp_dir().join(format!(
        "bg3_asset_export_{}_{}",
        std::process::id(),
        next_temp_id()
    ));
    let shared = temp_root.join("files");
    if let Err(err) = fs::create_dir_all(&shared) {
        push_warning(warnings, "vtStagingFailed", err.to_string());
    } else {
        for (seq, vt) in vt_targets.iter().enumerate() {
            progress.item(PHASE_VIRTUAL, Some(vt.name.clone()));

            let Some(matched) = match_for_hash(vt_matches, &vt.gtex_hash) else {
                push_warning(warnings, "vtGtpNotFound", vt.gtex_hash.clone());
                continue;
            };

            let stage = temp_root.join(format!("stage_{seq}"));
            let mut archive = lock_pool(pool)?;
            if let Err(err) = export_virtual_texture(
                &mut archive,
                matched,
                &vt.name,
                &vt_dir,
                &shared,
                &stage,
                convert_to_png,
                files,
                warnings,
            ) {
                push_warning(warnings, "vtFailed", format!("{}: {err}", vt.name));
            }
            // Released per virtual texture, so a long export keeps interleaving with previews
            drop(archive);
            let _ = fs::remove_dir_all(&stage);
        }
    }
    let _ = fs::remove_dir_all(&temp_root);

    Ok(())
}

fn next_temp_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Extract a single virtual texture: pull the page file `matched` names (plus the GTS beside it),
/// resolve the three layer DDS files, and write them to the export dir.
#[allow(clippy::too_many_arguments)]
fn export_virtual_texture(
    pak: &mut Archives,
    matched: &GtpMatch,
    vt_name: &str,
    vt_dir: &Path,
    shared: &Path,
    stage: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    fs::create_dir_all(stage).map_err(|e| format!("Failed to create staging directory: {e}"))?;

    // Which files the archives hold for this page file is `virtual_textures`' question; the export
    // only consumes the staged paths
    let StagedSources {
        gtp,
        gts_candidates,
    } = virtual_textures::stage_sources(pak, matched, shared)?;

    extract_with_any_gts(&gtp, gts_candidates, stage)?;

    let safe_name = sanitize_file_name(vt_name);
    collect_vt_layers(stage, vt_dir, &safe_name, convert_to_png, files, warnings)
}

/// Run the extractor with each GTS candidate until one accepts the page file: GTS naming does not
/// always match the GTP, so the candidates are tried in likelihood order. Trying them is safe
/// because the extractor checks the hash against the GTS metadata itself
fn extract_with_any_gts(
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
/// convert them when PNG was asked for (`try_write_png_and_record` drops the DDS in that case)
fn collect_vt_layers(
    stage: &Path,
    vt_dir: &Path,
    safe_name: &str,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    for layer in VT_LAYERS {
        // The extractor writes `<name>_<layer>.dds` (e.g. `..._basemap.dds`); the export file
        // drops the trailing `Map` from the layer name (`BaseMap` → `Base`), matching the
        // engine's `Albedo_Normal_Physical` naming for split virtual textures
        let export_suffix = layer.trim_end_matches("Map");
        let Some(src) = find_layer_output(stage, layer)? else {
            continue;
        };

        let stem = format!("{safe_name}_{export_suffix}");
        let dds_path = unique_path(vt_dir, &stem, "dds");
        if fs::rename(&src, &dds_path).is_err() {
            fs::copy(&src, &dds_path)
                .map_err(|e| format!("Failed to move {layer} output: {e}"))?;
        }

        let dds_bytes = fs::read(&dds_path).unwrap_or_default();
        if convert_to_png {
            match dds_bytes_to_png_bytes(&dds_bytes) {
                Ok(png) => {
                    let png_path = unique_path(vt_dir, &stem, "png");
                    if try_write_png_and_record(
                        &png,
                        &png_path,
                        &dds_path,
                        files,
                        warnings,
                        &stem,
                    ) {
                        continue;
                    }
                }
                Err(err) => push_warning(warnings, "pngConvertFailed", format!("{stem}: {err}")),
            }
        }

        files.push(ExportedFile {
            path: dds_path.display().to_string(),
            kind: "dds".to_string(),
            size_bytes: dds_bytes.len(),
        });
    }

    Ok(())
}

/// Find the extractor output for one layer; the file name ends with `_<layer>.dds`
fn find_layer_output(stage: &Path, layer: &str) -> Result<Option<PathBuf>, String> {
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

/// Assemble the `asset.json` content of one export
fn build_manifest(
    asset: &VisualAsset,
    vt_matches: &[GtpMatch],
    mesh_format: MeshFormat,
    files: &[ExportedFile],
) -> ExportManifest {
    ExportManifest {
        name: asset.name.clone(),
        path: asset.gr2_path.clone(),
        mesh_format,
        source: asset.source_pak.clone(),
        material_ids: asset.material_ids.clone(),
        textures: asset.textures.iter().map(TextureSummary::from).collect(),
        virtual_textures: asset
            .virtual_textures
            .iter()
            .map(|vt| VirtualTextureSummary::new(vt, match_for_hash(vt_matches, &vt.gtex_hash)))
            .collect(),
        files: files.to_vec(),
        exported_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        maclarian_version: maclarian::VERSION.to_string(),
    }
}

/// Write `asset.json`; the artifacts are already on disk by then, so a manifest failure is recorded
/// as a warning instead of failing the whole export
fn write_manifest(manifest: &ExportManifest, out_dir: &Path, warnings: &mut Vec<ExportWarning>) {
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

//! Asset export pipeline: exports a full visual asset (GR2 mesh + textures + virtual textures +
//! metadata) into a target directory.
//!
//! Format conversions all reuse maclarian: `convert_gr2_bytes_to_glb`, `dds_bytes_to_png_bytes`
//! and `VirtualTextureExtractor`. Archive access belongs to `crate::archives`, page file staging to
//! `crate::virtual_textures`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use maclarian::converter::dds_bytes_to_png_bytes;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use maclarian::virtual_texture::VirtualTextureExtractor;
use tempfile::TempDir;

use crate::archives::{lock_pool, Archives, Pak};
use crate::error::AppError;
use crate::models::{
    manifest_materials, match_for_hash, texture_summaries, virtual_texture_summaries,
    ExportManifest, ExportOptions, ExportProgress, ExportResult, ExportWarning, ExportedFile,
    MeshFormat, VirtualTextureSummary,
};
use crate::state::MaterialInfo;
use crate::virtual_textures::{
    self, PageFileSizeCache, StagedSources, PAGE_FILE_SIZE_CACHE_CAPACITY,
};

/// Progress phases (the frontend uses these to look up i18n copy)
const PHASE_PREPARE: &str = "prepare";
const PHASE_MODEL: &str = "model";
/// Raw GR2 copy (no conversion happening, so the frontend shows different copy)
const PHASE_MODEL_RAW: &str = "modelRaw";
const PHASE_TEXTURES: &str = "textures";
const PHASE_VIRTUAL: &str = "virtualTextures";
const PHASE_MANIFEST: &str = "manifest";
const PHASE_DONE: &str = "done";

/// The three layers exported from a virtual texture (order matches `VirtualTextureLayer`); the
/// engine calls the base layer `Albedo` (see `export_layer_name`).
const VT_LAYERS: [&str; 3] = ["BaseMap", "NormalMap", "PhysicalMap"];

/// Append a structured warning (code is localized by the frontend, detail keeps the raw message)
fn push_warning(warnings: &mut Vec<ExportWarning>, code: &str, detail: impl Into<String>) {
    warnings.push(ExportWarning {
        code: code.to_string(),
        detail: detail.into(),
    });
}

/// Sanitize a file/directory name: `sanitize-filename` strips forbidden characters and reserved
/// names, then the result is capped at 80 chars and falls back to `asset` when empty.
fn sanitize_file_name(raw: &str) -> String {
    let cleaned = sanitize_filename::sanitize_with_options(
        // Leading/trailing dots are trimmed here rather than by the library, which would turn a
        // trailing `.` into `_` because of the replacement below
        raw.trim().trim_matches('.'),
        sanitize_filename::Options {
            // Keep the `_` substitution the export has always used instead of the default removal
            replacement: "_",
            ..Default::default()
        },
    );

    if cleaned.is_empty() {
        "asset".to_string()
    } else {
        cleaned.chars().take(80).collect()
    }
}

/// Record one written artifact in the export result
fn record_file(files: &mut Vec<ExportedFile>, path: &Path, kind: &str, size_bytes: usize) {
    files.push(ExportedFile {
        path: path.display().to_string(),
        kind: kind.to_string(),
        size_bytes,
    });
}

/// Write a PNG, drop the intermediate DDS on success and record the artifact; returns whether the
/// PNG has been written.
fn try_write_png_and_record(
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

/// Write the PNG form of `dds_path` and drop the DDS on success; `false` leaves the DDS in place.
fn replace_dds_with_png(
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
            push_warning(warnings, "pngConvertFailed", format!("{source_label}: {err}"));
            false
        }
    }
}

/// Tracks export progress: counts finished items and reports the running percentage.
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

    /// Report a phase boundary without counting an item (`prepare` / `done`).
    fn phase(&self, phase: &str, percent: f32) {
        (self.on_progress)(ExportProgress {
            phase: phase.to_string(),
            current_file: None,
            percent,
        });
    }
}

/// Everything decided before the first artifact is written: where it goes, in which form, and how
/// many items the progress bar counts.
struct ExportPlan<'a> {
    dir_name: String,
    out_dir: PathBuf,
    mesh_format: MeshFormat,
    export_textures: bool,
    convert_to_png: bool,
    vt_targets: Vec<&'a VirtualTextureRef>,
    progress_total: usize,
}

impl ExportPlan<'_> {
    /// Where the extracted layers of the virtual textures go
    fn vt_dir(&self) -> PathBuf {
        self.out_dir.join("virtual_textures")
    }
}

/// Decide what the export will contain and create its output directory
fn plan_export<'a>(
    asset: &'a VisualAsset,
    dest_root: &Path,
    options: &ExportOptions,
) -> Result<ExportPlan<'a>, AppError> {
    // Textures are always exported separately (regular + virtual); the format is a single switch
    let export_textures = options.texture_format.is_export();

    let dir_name = sanitize_file_name(&asset.name);
    let out_dir = dest_root.join(&dir_name);
    fs::create_dir_all(&out_dir).map_err(|e| AppError::CreateDirectory {
        target: "export",
        detail: e.to_string(),
    })?;

    let vt_targets = vt_targets_of(asset, export_textures);
    let texture_total = if export_textures {
        asset.textures.len()
    } else {
        0
    };
    // 1 mesh + 1 manifest, plus texture files and virtual textures
    let progress_total = 2 + texture_total + vt_targets.len();

    Ok(ExportPlan {
        dir_name,
        out_dir,
        mesh_format: options.mesh_format,
        export_textures,
        convert_to_png: options.texture_format.is_png(),
        vt_targets,
        progress_total,
    })
}

/// Page files worth extracting: extracting needs a hash to resolve the `GtpMatch` naming the page
/// file in the archives, and virtual textures are only exported together with the textures
fn vt_targets_of(asset: &VisualAsset, export_textures: bool) -> Vec<&VirtualTextureRef> {
    if !export_textures {
        return Vec::new();
    }
    asset
        .virtual_textures
        .iter()
        .filter(|vt| !vt.gtex_hash.trim().is_empty())
        .collect()
}

/// Export a single visual asset. A mesh failure aborts the export; a texture or virtual texture
/// failure only records a warning.
///
/// `materials` are the cache entries of this asset's materials, needed to name the manifest rows.
pub fn run_export(
    asset: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    pool: &Arc<Mutex<Archives>>,
    dest_root: &Path,
    options: &ExportOptions,
    vt_matches: &[GtpMatch],
    on_progress: &dyn Fn(ExportProgress),
) -> Result<ExportResult, AppError> {
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
    export_virtual_textures(pool, &plan, vt_matches, &mut files, &mut warnings, &mut progress)?;

    // 4. Metadata manifest (always written, but never listed among the exported files)
    progress.item(PHASE_MANIFEST, None);
    let mut manifest = build_manifest(asset, materials, vt_matches);
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

/// Read the mesh and write it into the export directory; any failure here aborts the export.
fn export_mesh(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    files: &mut Vec<ExportedFile>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), AppError> {
    progress.item(
        if plan.mesh_format.is_glb() {
            PHASE_MODEL
        } else {
            PHASE_MODEL_RAW
        },
        Some(asset.gr2_path.clone()),
    );

    let mesh_bytes = read_mesh_bytes(asset, pool, plan.mesh_format)?;

    let mesh_ext = plan.mesh_format.extension();
    let mesh_path = plan.out_dir.join(format!("{}.{mesh_ext}", plan.dir_name));
    fs::write(&mesh_path, &mesh_bytes).map_err(|e| AppError::MeshWrite {
        format: mesh_ext,
        detail: e.to_string(),
    })?;
    record_file(files, &mesh_path, mesh_ext, mesh_bytes.len());

    Ok(())
}

/// Pull the mesh bytes out of the archives, converted to GLB unless the raw GR2 was asked for
fn read_mesh_bytes(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    mesh_format: MeshFormat,
) -> Result<Vec<u8>, AppError> {
    // The mesh is read from `Models.pak` and nowhere else; a GR2 the game ships in another archive
    // reports that rather than being searched for
    let gr2_bytes = lock_pool(pool)?
        .read_from(Pak::Models, &asset.gr2_path)
        .map_err(|err| AppError::MeshUnavailable {
            detail: err.to_string(),
        })?;

    match mesh_format {
        // Raw GR2: the archive bytes are the artifact, nothing to convert
        MeshFormat::Gr2 => Ok(gr2_bytes),
        MeshFormat::Glb => {
            convert_gr2_bytes_to_glb(&gr2_bytes).map_err(|e| AppError::MeshConvert {
                detail: e.to_string(),
            })
        }
    }
}

/// Pull every regular texture out of the archives, optionally converting to PNG; a missing texture
/// is a warning.
fn export_textures(
    asset: &VisualAsset,
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), AppError> {
    // Regular textures are optional: nothing to do when only the mesh was asked for, or when the
    // asset has no material textures at all
    if !plan.export_textures || asset.textures.is_empty() {
        return Ok(());
    }

    let tex_dir = plan.out_dir.join("textures");
    fs::create_dir_all(&tex_dir).map_err(|e| AppError::CreateDirectory {
        target: "textures",
        detail: e.to_string(),
    })?;

    for tex in &asset.textures {
        progress.item(PHASE_TEXTURES, Some(tex.dds_path.clone()));
        export_texture(tex, pool, &tex_dir, plan.convert_to_png, files, warnings)?;
    }

    Ok(())
}

/// Read one texture out of the archives and write it into `tex_dir`
fn export_texture(
    tex: &TextureRef,
    pool: &Arc<Mutex<Archives>>,
    tex_dir: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), AppError> {
    // Locked per file only: it leaves the pool available to other commands while the export runs
    match lock_pool(pool)?.read_from(Pak::Textures, &tex.dds_path) {
        Ok(dds) => write_texture(tex, &dds, tex_dir, convert_to_png, files, warnings),
        Err(err) => push_warning(warnings, "textureUnavailable", err.to_string()),
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
    let stem = texture_stem(tex);
    // Same name, same place: a re-export overwrites the file the previous one left behind
    let dds_path = tex_dir.join(format!("{stem}.dds"));
    if let Err(err) = fs::write(&dds_path, dds) {
        push_warning(warnings, "textureWriteFailed", format!("{}: {err}", tex.dds_path));
        return;
    }

    let replaced = convert_to_png
        && replace_dds_with_png(dds, tex_dir, &stem, &dds_path, files, warnings, &tex.dds_path);
    if !replaced {
        record_file(files, &dds_path, "dds", dds.len());
    }
}

/// File stem of a texture: named after the actual DDS resource in the archive (e.g. `Body_BM`),
/// never after the material parameter slot (e.g. `ColorTexture`) or the bank display name
fn texture_stem(tex: &TextureRef) -> String {
    sanitize_file_name(
        Path::new(&tex.dds_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_else(|| tex.parameter_name.as_deref().unwrap_or(&tex.name)),
    )
}

/// Extract every virtual texture of the asset. All share one staging directory, so a GTS is read
/// once; each gets its own output directory.
fn export_virtual_textures(
    pool: &Arc<Mutex<Archives>>,
    plan: &ExportPlan<'_>,
    vt_matches: &[GtpMatch],
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
    progress: &mut ProgressTracker<'_>,
) -> Result<(), AppError> {
    if plan.vt_targets.is_empty() {
        return Ok(());
    }

    let vt_dir = plan.vt_dir();
    fs::create_dir_all(&vt_dir).map_err(|e| AppError::CreateDirectory {
        target: "virtual textures",
        detail: e.to_string(),
    })?;

    let staging = match VtStaging::new() {
        Ok(staging) => staging,
        Err(err) => {
            push_warning(warnings, "vtStagingFailed", err);
            return Ok(());
        }
    };
    if let Err(err) = staging.prepare() {
        push_warning(warnings, "vtStagingFailed", err);
        return Ok(());
    }

    for (seq, vt) in plan.vt_targets.iter().enumerate() {
        progress.item(PHASE_VIRTUAL, Some(vt.name.clone()));
        export_vt_target(pool, vt, vt_matches, seq, &staging, plan, files, warnings)?;
    }

    Ok(())
}

/// Extract one virtual texture in its own staging directory; a failure is only a warning.
#[allow(clippy::too_many_arguments)]
fn export_vt_target(
    pool: &Arc<Mutex<Archives>>,
    vt: &VirtualTextureRef,
    vt_matches: &[GtpMatch],
    seq: usize,
    staging: &VtStaging,
    plan: &ExportPlan<'_>,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), AppError> {
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

/// Staging area of one export run: page files share `files`, each output goes to `stage_N`, and
/// the `TempDir` removes it all when the run ends.
struct VtStaging {
    root: PathBuf,
    _temp: TempDir,
}

impl VtStaging {
    fn new() -> Result<Self, AppError> {
        let temp = tempfile::Builder::new()
            .prefix("bg3_asset_export_")
            .tempdir()
            .map_err(|e| AppError::CreateDirectory {
                target: "staging",
                detail: e.to_string(),
            })?;
        Ok(Self {
            root: temp.path().to_path_buf(),
            _temp: temp,
        })
    }

    /// Files staged for all page files (`virtual_textures::stage_sources` reuses them by name)
    fn shared(&self) -> PathBuf {
        self.root.join("files")
    }

    /// Output directory of the page file with this export sequence number
    fn stage(&self, seq: usize) -> PathBuf {
        self.root.join(format!("stage_{seq}"))
    }

    fn prepare(&self) -> Result<(), AppError> {
        // The staging root is a temp directory the OS just created, so a failure here is reported
        // as-is instead of through one of the `CreateDirectory` messages
        fs::create_dir_all(self.shared()).map_err(|e| AppError::Message(e.to_string()))
    }
}

/// Extract one virtual texture: stage the page file and GTS, then write the three layer DDS files.
#[allow(clippy::too_many_arguments)]
fn export_virtual_texture(
    pak: &mut Archives,
    matched: &GtpMatch,
    vt_name: &str,
    seq: usize,
    plan: &ExportPlan<'_>,
    staging: &VtStaging,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), AppError> {
    let stage = staging.stage(seq);
    fs::create_dir_all(&stage).map_err(|e| AppError::CreateDirectory {
        target: "staging",
        detail: e.to_string(),
    })?;

    // Which files the archives hold for this page file is `virtual_textures`' question; the export
    // only consumes the staged paths
    let StagedSources {
        gtp,
        gts_candidates,
    } = virtual_textures::stage_sources(pak, matched, &staging.shared())?;

    extract_with_any_gts(&gtp, gts_candidates, &stage)?;

    let safe_name = sanitize_file_name(vt_name);
    collect_vt_layers(&stage, &plan.vt_dir(), &safe_name, plan.convert_to_png, files, warnings)
}

/// Run the extractor with each GTS candidate until one accepts the page file; the extractor checks
/// the hash itself, so trying them is safe.
fn extract_with_any_gts(
    gtp: &Path,
    gts_candidates: Vec<PathBuf>,
    stage: &Path,
) -> Result<(), AppError> {
    let mut last_err = "no GTS candidate available".to_string();
    for gts_path in gts_candidates {
        match VirtualTextureExtractor::extract_with_gts(gtp, &gts_path, stage) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = err.to_string();
                // Clear partial output so the next candidate's artifacts do not mix together
                let _ = fs::remove_dir_all(stage);
                fs::create_dir_all(stage).map_err(|e| AppError::RecreateStagingDirectory {
                    detail: e.to_string(),
                })?;
            }
        }
    }

    Err(AppError::VirtualTextureExtraction { detail: last_err })
}

/// Move the extracted layer files into `vt_dir`, renamed after the asset and converted to PNG when
/// asked.
fn collect_vt_layers(
    stage: &Path,
    vt_dir: &Path,
    safe_name: &str,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), AppError> {
    for layer in VT_LAYERS {
        let Some(src) = find_layer_output(stage, layer)? else {
            continue;
        };
        export_vt_layer(layer, &src, vt_dir, safe_name, convert_to_png, files, warnings)?;
    }

    Ok(())
}

/// The name a layer takes in the export: `BaseMap` → `Albedo`, others drop the trailing `Map`.
fn export_layer_name(layer: &str) -> &str {
    match layer {
        "BaseMap" => "Albedo",
        other => other.trim_end_matches("Map"),
    }
}

/// Move one extracted layer into `vt_dir` under its export name and record the artifact
fn export_vt_layer(
    layer: &'static str,
    src: &Path,
    vt_dir: &Path,
    safe_name: &str,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), AppError> {
    // The extractor writes `<name>_<layer>.dds`; the export names it after the engine's layer.
    // A rename onto an existing destination fails on Windows, hence the copy fallback
    let stem = format!("{safe_name}_{}", export_layer_name(layer));
    // Same name, same place: a re-export overwrites the previous file
    let dds_path = vt_dir.join(format!("{stem}.dds"));
    if fs::rename(src, &dds_path).is_err() {
        fs::copy(src, &dds_path).map_err(|e| AppError::MoveLayerOutput {
            layer,
            detail: e.to_string(),
        })?;
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
fn find_layer_output(stage: &Path, layer: &str) -> Result<Option<PathBuf>, AppError> {
    let suffix = format!("_{}.dds", layer.to_lowercase());
    Ok(fs::read_dir(stage)
        .map_err(|e| AppError::ListExtractionOutput {
            detail: e.to_string(),
        })?
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

/// Fill in the pixel size of every virtual texture the manifest carries.
///
/// The size comes from `virtual_textures::page_file_size`, so `asset.json` agrees with the detail
/// view; an unavailable pool or unparsable GTS leaves it unset rather than failing an export.
fn fill_vt_sizes(pool: &Arc<Mutex<Archives>>, vt_matches: &[GtpMatch], manifest: &mut ExportManifest) {
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

    // One GTS serves every page file of its tile set, so a shared cache reads each one once
    let sizes = PageFileSizeCache::new(PAGE_FILE_SIZE_CACHE_CAPACITY);
    for material in &mut manifest.materials {
        for row in &mut material.virtual_textures {
            fill_vt_size(&mut archives, vt_matches, &sizes, row);
        }
    }
}

/// Size of the page file one row resolved to, or nothing when it did not resolve
fn fill_vt_size(
    archives: &mut Archives,
    vt_matches: &[GtpMatch],
    sizes: &PageFileSizeCache,
    row: &mut VirtualTextureSummary,
) {
    let size = match_for_hash(vt_matches, &row.hash)
        .and_then(|matched| virtual_textures::page_file_size(archives, sizes, matched, &row.hash));
    row.set_size(size);
}

/// Assemble the `asset.json` content; `fill_vt_sizes` settles the virtual texture sizes before it
/// is written.
fn build_manifest(
    asset: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    vt_matches: &[GtpMatch],
) -> ExportManifest {
    // Built once and handed to the material rows, which carry these very rows for their resources
    let textures = texture_summaries(asset, materials);
    let virtual_textures = virtual_texture_summaries(asset, vt_matches, materials);

    ExportManifest {
        id: asset.id.clone(),
        name: asset.name.clone(),
        path: asset.gr2_path.clone(),
        materials: manifest_materials(asset, materials, &textures, &virtual_textures),
        exported_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        maclarian_version: maclarian::VERSION.to_string(),
    }
}

/// Write `asset.json`; a manifest failure is a warning, as the artifacts are already on disk.
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

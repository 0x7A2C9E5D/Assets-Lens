//! Asset export pipeline: exports a full visual asset (GR2 mesh + textures + virtual textures +
//! metadata) into a target directory.
//!
//! All format conversions reuse maclarian instead of reimplementing anything:
//! - `convert_gr2_bytes_to_glb`: GR2 → GLB (mesh only, no embedded textures)
//! - `dds_bytes_to_png_bytes`: DDS → PNG (direct in-memory conversion, no intermediate files)
//! - `VirtualTextureExtractor`: GTP + GTS → three layer DDS files (BaseMap / NormalMap / PhysicalMap)
//! - `PakReaderCache` / `PakOperations`: maclarian's own readers pull GR2 / DDS / GTP / GTS straight
//!   out of the archives (no full temporary extraction), and its table cache keeps the parsed file
//!   indexes resident, so a lookup costs a table scan instead of reparsing an index per file

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::converter::dds_bytes_to_png_bytes;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GtpMatch, MergedResolver, VirtualTextureRef, VisualAsset};
use maclarian::pak::{PakOperations, PakReaderCache};
use maclarian::virtual_texture::{VirtualTextureExtractor, get_subfolder_name};

use crate::models::{
    ExportManifest, ExportOptions, ExportProgress,
    ExportResult, ExportWarning, ExportedFile, MeshFormat, TextureSummary,
    VirtualTextureSummary, match_for_hash,
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

/// True when `file_name` is a LSPK data-partition archive (`<Name>_<n>.pak`, e.g.
/// `VirtualTextures_12.pak`). BG3 splits large resources over numbered archives that only hold
/// raw data blocks — they carry no LSPK header of their own, cannot be opened standalone, and are
/// already reachable through their main (`<Name>.pak`) archive, so they are excluded up front.
fn is_data_partition(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    let stem = lower.strip_suffix(".pak").unwrap_or(&lower);
    match stem.rsplit_once('_') {
        Some((_, tail)) => !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// List every main `.pak` under the data directory in file-name order. Numbered data partitions
/// (`<Name>_<n>.pak`) are excluded — see `is_data_partition`.
///
/// No archive is promoted: meshes, textures and virtual textures are all looked up the same way,
/// so a "preferred" archive would only decide which copy wins when a path exists in several
/// archives, and never which archives get searched.
pub fn main_paks(game_path: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paks: Vec<PathBuf> = fs::read_dir(game_path)
        .map_err(|e| format!("Failed to list BG3 data directory: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|s| s.to_str()) == Some("pak")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_data_partition)
        })
        .collect();

    // Deterministic order only: `read_dir` order is arbitrary, and which archive answers a lookup
    // has to stay stable between runs.
    paks.sort();
    Ok(paks)
}

/// Which archive holds a mesh or a texture: the only two file kinds whose archive is ever named (the
/// detail panel and `asset.json`), and therefore the only two that are indexed.
///
/// Built with maclarian's own API: its `extract_dds_textures` answers the same question the same
/// way, by listing every archive with `PakOperations::list` and testing containment. The crate has
/// no cheaper reverse lookup — `PakReaderCache` keeps its tables private, and its parser never fills
/// the `source_pak` fields — and listing an archive costs a full table read.
///
/// The tables' own spelling is the key, which is exactly how the database spells its paths. The
/// first archive to list a path wins, i.e. the order `read_file` sweeps in, so a label can never
/// contradict where the bytes actually come from.
///
/// Filling the database's `source_pak` fields is all this is for, so it is built after a database
/// build and dropped right after (`AppState::fill_source_paks`) — holding ~40 MB for the whole
/// session to answer lookups that the filled fields already answer would be waste. A full installation
/// holds 224560 meshes and textures out of 567681 entries; indexing the rest (sound banks, layouts,
/// virtual texture pages) would triple the peak for paths nobody asks about, and storing the archive
/// name per entry instead of an index into `names` would double it again.
pub struct PakIndex {
    names: Vec<String>,
    by_path: HashMap<Box<str>, u16>,
}

impl PakIndex {
    /// Archive file name holding `target`, spelled the way the file tables spell it: `/` separators,
    /// original casing. A record spelled with `\` separators is retried once.
    pub fn name_of(&self, target: &str) -> Option<&str> {
        self.lookup(target)
            .or_else(|| self.lookup(&target.replace('\\', "/")))
    }

    fn lookup(&self, target: &str) -> Option<&str> {
        let slot = *self.by_path.get(target)?;
        self.names.get(slot as usize).map(String::as_str)
    }
}

/// Whether `path` names a mesh or a texture, i.e. something whose archive is named (see `PakIndex`)
fn is_mesh_or_texture(path: &str) -> bool {
    path.get(path.len().saturating_sub(4)..).is_some_and(|ext| {
        ext.eq_ignore_ascii_case(".gr2") || ext.eq_ignore_ascii_case(".dds")
    })
}

/// List every main archive and record which one holds which mesh / texture path
pub fn build_pak_index(paks: &[PathBuf]) -> PakIndex {
    let names: Vec<String> = paks
        .iter()
        .map(|pak| {
            pak.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    let mut by_path: HashMap<Box<str>, u16> = HashMap::new();
    // A full installation has a couple of dozen archives, so the index fits in a `u16` with room to spare
    for (slot, pak) in paks.iter().enumerate() {
        match PakOperations::list(pak) {
            Ok(entries) => {
                for path in entries {
                    if is_mesh_or_texture(&path) {
                        by_path.entry(path.into_boxed_str()).or_insert(slot as u16);
                    }
                }
            }
            Err(err) => eprintln!("[maclarian] listing {} failed: {err}", pak.display()),
        }
    }

    PakIndex { names, by_path }
}

/// Lock maclarian's shared PAK table cache for a single read
pub fn lock_cache(
    cache: &Arc<Mutex<PakReaderCache>>,
) -> Result<MutexGuard<'_, PakReaderCache>, String> {
    cache
        .lock()
        .map_err(|e| format!("PAK cache lock unavailable: {e}"))
}

/// Read a file from whichever main archive holds it.
///
/// maclarian's own `read_files_bulk` does the work instead of a hand-rolled table walk: the database
/// hands out exactly the spelling the archives use (`Generated/Public/...`, `/`-separated, original
/// casing — verified against a full install), which is what that reader compares against. Its table
/// cache turns the sweep into a sequence of table scans rather than reparsing an index per file, and
/// the first archive that answers wins, exactly as before.
pub fn read_file(
    cache: &mut PakReaderCache,
    paks: &[PathBuf],
    target: &str,
) -> Result<Vec<u8>, String> {
    if let Some(bytes) = scan_for(cache, paks, target) {
        return Ok(bytes);
    }

    // Fallback for a record that spells separators the Windows way; the archives always use `/`
    let slashed = target.replace('\\', "/");
    if slashed != target {
        if let Some(bytes) = scan_for(cache, paks, &slashed) {
            return Ok(bytes);
        }
    }

    // Last resort: the name differs only in casing. The file tables are authoritative for spelling,
    // so match them case-insensitively and read back with the table's own spelling
    let want = normalize_path(target);
    for pak in paks {
        if let Ok(entries) = PakOperations::list(pak) {
            if let Some(exact) = entries.iter().find(|path| normalize_path(path) == want) {
                return read_from(cache, pak, exact);
            }
        }
    }

    Err(format!("{target} not found in BG3 archives"))
}

/// Sweep the archives for one exact path spelling. Archives that do not hold the file answer with an
/// empty map, and archives that cannot be read are skipped the same way.
fn scan_for(cache: &mut PakReaderCache, paks: &[PathBuf], target: &str) -> Option<Vec<u8>> {
    for pak in paks {
        if let Ok(mut found) = cache.read_files_bulk(pak, &[target]) {
            if let Some(bytes) = found.remove(target) {
                return Some(bytes);
            }
        }
    }
    None
}

/// Read a file from one specific archive.
///
/// The virtual texture pipeline never sweeps the data directory: it reads the archive maclarian's
/// lookup recorded for the match (`GtpMatch::pak_path`) — on a full install `VirtualTextures.pak`,
/// the only archive holding a single `.gtp` or `.gts` (verified against a full install).
pub fn read_from(cache: &mut PakReaderCache, pak: &Path, target: &str) -> Result<Vec<u8>, String> {
    let mut found = cache
        .read_files_bulk(pak, &[target])
        .map_err(|e| format!("Failed to read {}: {e}", pak.display()))?;
    found
        .remove(target)
        .ok_or_else(|| format!("{target} not found in {}", pak.display()))
}

/// Resolve the `.gtp` page file behind each virtual texture hash, with maclarian's own lookup
/// (`MergedResolver::find_gtp_by_hashes_in_pak`): it lists the archive's file table and matches the
/// 32-char hash against the page file names — the only way to learn where a virtual texture really
/// lives, since `VirtualTextureRef` carries nothing but that hash.
///
/// `search_pak` is the archive handed to the lookup, and every match it returns records that archive
/// in `GtpMatch::pak_path`. From there on the archive travels inside the match — the GTP read, the GTS
/// lookup and the manifest's archive name all take it from that field — so the archive is chosen once,
/// here, and never rebuilt from an archive's name. maclarian's `find_gtp_by_hashes` (no pak argument)
/// would go through `virtual_textures_pak_path()`, which rebuilds the path from `bg3_data_path()`'s
/// auto-detection and finds nothing whenever the data directory was pointed at by hand.
///
/// The hashes variant rather than `find_virtual_textures_for_visual_in_pak`: that one looks the
/// visual up by *name*, and names are not unique in this database (the GUID is the identity), so it
/// could answer with another visual's page files. Both hand back the same `GtpMatch`.
///
/// An empty result is the honest outcome for an archive that cannot be listed or holds no page file
/// for a hash: virtual textures are an optional artifact and must not abort an export.
pub fn find_vt_matches(
    resolver: &MergedResolver,
    hashes: &[&str],
    search_pak: &Path,
) -> Vec<GtpMatch> {
    // The comparison against the file names is case-sensitive, while the database's hashes are only
    // lowercase hex by convention
    let wanted: Vec<String> = hashes
        .iter()
        .map(|hash| hash.trim().to_ascii_lowercase())
        .filter(|hash| !hash.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let wanted: Vec<&str> = wanted.iter().map(String::as_str).collect();

    match resolver.find_gtp_by_hashes_in_pak(&wanted, search_pak) {
        Ok(matches) => matches,
        Err(err) => {
            eprintln!("[maclarian] listing {} failed: {err}", search_pak.display());
            Vec::new()
        }
    }
}

/// Normalize a path for comparison: unify on `/` separators and lowercase.
/// Only used to reconcile a record whose spelling differs from the archive's own (see `read_file`).
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
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

/// Export a single visual asset. The GLB is the core artifact — its failure aborts the whole
/// export, while a single texture / virtual texture failure only records a warning.
///
/// `vt_matches` are the `.gtp` page files maclarian resolved for this asset's virtual texture
/// hashes (`AppState::vt_matches`), so neither the export nor `asset.json` has to scan an archive
/// for them.
pub fn run_export(
    asset: &VisualAsset,
    cache: &Arc<Mutex<PakReaderCache>>,
    paks: &[PathBuf],
    vt_matches: &[GtpMatch],
    dest_root: &Path,
    options: &ExportOptions,
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
    let mut done = 0usize;

    let mut emit = |phase: &str, file: Option<String>| {
        done += 1;
        on_progress(ExportProgress {
            phase: phase.to_string(),
            current_file: file,
            percent: if total == 0 {
                1.0
            } else {
                done as f32 / total as f32
            },
        });
    };

    on_progress(ExportProgress {
        phase: PHASE_PREPARE.to_string(),
        current_file: None,
        percent: 0.0,
    });

    // ---- 1. Mesh: raw GR2 straight out of the PAK, or a plain GR2 → GLB conversion ----
    let mesh_format = options.mesh_format;
    emit(
        if mesh_format.is_glb() {
            PHASE_MODEL
        } else {
            PHASE_MODEL_RAW
        },
        Some(asset.gr2_path.clone()),
    );
    let gr2_bytes = read_file(&mut *lock_cache(cache)?, paks, &asset.gr2_path)
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
    files.push(ExportedFile {
        path: mesh_path.display().to_string(),
        kind: mesh_ext.to_string(),
        size_bytes: mesh_bytes.len(),
    });

    // ---- 2. Textures: pull DDS from PAKs, optionally converting to PNG ----
    if extract_textures && !asset.textures.is_empty() {
        let tex_dir = out_dir.join("textures");
        fs::create_dir_all(&tex_dir).map_err(|e| format!("Failed to create textures directory: {e}"))?;

        for tex in &asset.textures {
            emit(PHASE_TEXTURES, Some(tex.dds_path.clone()));

            // Locked per file only: decompressing one texture is quick, and it leaves the cache
            // available to other commands (a preview) while the export runs
            let dds = read_file(&mut *lock_cache(cache)?, paks, &tex.dds_path);
            match dds {
                Ok(dds) => {
                    // Name files after the actual DDS resource in the archive (e.g. `Body_BM`),
                    // never the material parameter slot (e.g. `ColorTexture`) or bank display name
                    let stem = sanitize_file_name(
                        Path::new(&tex.dds_path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or_else(|| {
                                tex.parameter_name.as_deref().unwrap_or(&tex.name)
                            }),
                    );
                    let dds_path = unique_path(&tex_dir, &stem, "dds");
                    if let Err(err) = fs::write(&dds_path, &dds) {
                        push_warning(
                            &mut warnings,
                            "textureWriteFailed",
                            format!("{}: {err}", tex.dds_path),
                        );
                        continue;
                    }

                    if convert_to_png {
                        match dds_bytes_to_png_bytes(&dds) {
                            Ok(png) => {
                                let png_path = unique_path(&tex_dir, &stem, "png");
                                if try_write_png_and_record(
                                    &png,
                                    &png_path,
                                    &dds_path,
                                    &mut files,
                                    &mut warnings,
                                    &tex.dds_path,
                                ) {
                                    continue;
                                }
                            }
                            Err(err) => push_warning(
                                &mut warnings,
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
                Err(err) => push_warning(&mut warnings, "textureUnavailable", err),
            }
        }
    }

    // ---- 3. Virtual textures: GTP + GTS → three layer DDS files ----
    // Every virtual texture is read out of the archive its `GtpMatch` came from: that archive and the
    // page file's own path are the extraction's whole input. A hash maclarian's lookup did not resolve
    // (no archive, or no page file naming it) reports as not found.
    if !vt_targets.is_empty() {
        let vt_dir = out_dir.join("virtual_textures");
        fs::create_dir_all(&vt_dir)
            .map_err(|e| format!("Failed to create virtual textures directory: {e}"))?;

        let temp_root = std::env::temp_dir().join(format!(
            "bg3_asset_export_{}_{}",
            std::process::id(),
            next_temp_id()
        ));
        for (seq, vt) in vt_targets.iter().enumerate() {
            emit(PHASE_VIRTUAL, Some(vt.name.clone()));

            let Some(matched) = match_for_hash(vt_matches, &vt.gtex_hash) else {
                push_warning(&mut warnings, "vtGtpNotFound", vt.gtex_hash.clone());
                continue;
            };

            // A directory per virtual texture: the extraction writes its three layers there, and the
            // next one starts from an empty directory even if this one failed halfway
            let stage = temp_root.join(format!("stage_{seq}"));
            let mut guard = lock_cache(cache)?;
            if let Err(err) = export_virtual_texture(
                &mut guard,
                &matched.pak_path,
                &matched.gtp_path,
                &vt.name,
                &vt_dir,
                &stage,
                convert_to_png,
                &mut files,
                &mut warnings,
            ) {
                push_warning(&mut warnings, "vtFailed", format!("{}: {err}", vt.name));
            }
            // Released per virtual texture, so a long export keeps interleaving with previews
            drop(guard);
            let _ = fs::remove_dir_all(&stage);
        }
        let _ = fs::remove_dir_all(&temp_root);
    }

    // ---- 4. Metadata manifest (always written, but never listed among the exported files) ----
    emit(PHASE_MANIFEST, None);
    let manifest = ExportManifest {
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
        files: files.clone(),
        exported_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        maclarian_version: maclarian::VERSION.to_string(),
    };

    let manifest_path = out_dir.join("asset.json");
    match serde_json::to_string_pretty(&manifest) {
        Ok(json) => {
            if let Err(err) = fs::write(&manifest_path, json) {
                push_warning(&mut warnings, "manifestWriteFailed", err.to_string());
            }
        }
        Err(err) => push_warning(&mut warnings, "manifestSerializeFailed", err.to_string()),
    }

    on_progress(ExportProgress {
        phase: PHASE_DONE.to_string(),
        current_file: None,
        percent: 1.0,
    });

    Ok(ExportResult {
        output_dir: out_dir.display().to_string(),
        files,
        warnings,
    })
}

fn next_temp_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Extract a single virtual texture: `source` is the archive its page file lives in and `path` is
/// that page file — those two are the whole input, both halves of the pair are read from `source`.
///
/// The sidecar needs no lookup: BG3 names it after the tileset that the page file's own name carries
/// (`..._3_<hash>.gtp` ← `..._3.gts`, the very convention maclarian's own GTS lookup follows), and
/// the GTex hash is read out of the page file's name as well. Both files are staged into `stage`
/// first, because the extraction reads them from disk.
#[allow(clippy::too_many_arguments)]
fn export_virtual_texture(
    cache: &mut PakReaderCache,
    source: &Path,
    path: &str,
    vt_name: &str,
    vt_dir: &Path,
    stage: &Path,
    convert_to_png: bool,
    files: &mut Vec<ExportedFile>,
    warnings: &mut Vec<ExportWarning>,
) -> Result<(), String> {
    fs::create_dir_all(stage).map_err(|e| format!("Failed to create staging directory: {e}"))?;

    let gtp_path = stage.join(file_name(path)?);
    fs::write(&gtp_path, read_from(cache, source, path)?)
        .map_err(|e| format!("Failed to stage GTP: {e}"))?;

    // The sidecar shares the page file's own name: BG3 appends the GTex hash to a tileset name, and
    // maclarian's `get_subfolder_name` strips exactly that hash back off
    let gts_rel = format!("{}.gts", get_subfolder_name(path));
    let gts_path = stage.join(file_name(&gts_rel)?);
    fs::write(&gts_path, read_from(cache, source, &gts_rel)?)
        .map_err(|e| format!("Failed to stage GTS: {e}"))?;

    VirtualTextureExtractor::extract_with_gts(&gtp_path, &gts_path, stage)
        .map_err(|err| format!("Virtual texture extraction failed: {err}"))?;

    let safe_name = sanitize_file_name(vt_name);
    for layer in VT_LAYERS {
        // The extractor writes `<name>_<layer>.dds` (e.g. `..._basemap.dds`); the export file
        // drops the trailing `Map` from the layer name (`BaseMap` → `Base`), matching the
        // engine's `Albedo_Normal_Physical` naming for split virtual textures
        let export_suffix = layer.trim_end_matches("Map");
        let suffix = format!("_{}.dds", layer.to_lowercase());
        let Some(src) = fs::read_dir(stage)
            .map_err(|e| format!("Failed to list extraction output: {e}"))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase().ends_with(&suffix))
        else {
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

/// The last segment of a path inside an archive, e.g. the `Albedo_Normal_Physical_3_<hash>.gtp` of
/// `Generated/Public/VirtualTextures/Albedo_Normal_Physical_3_<hash>.gtp`
fn file_name(path: &str) -> Result<&str, String> {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid page file path: {path}"))
}

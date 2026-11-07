use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GameDataResolver, MergedDatabase, VisualAsset};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::archives::{lock_pool, Pak};
use crate::error::AppError;
use crate::export::run_export;
use crate::models::{
    match_for_hash, materials_of, AppInfo, BuildProgress, DatabaseStats, ExportOptions,
    ExportProgress, ExportResult, ModelPreview, Page, VisualAssetDetail, VisualSummary,
};
use crate::state::{extract_materials, fill_virtual_texture_parameters, AppState};
use crate::virtual_texture_params;
use crate::virtual_textures;

pub type SharedState = Arc<Mutex<AppState>>;

/// Lock the shared state; the only failure mode is a poisoned mutex
fn lock(state: &SharedState) -> Result<MutexGuard<'_, AppState>, AppError> {
    state.lock().map_err(|e| AppError::StateLock {
        detail: e.to_string(),
    })
}

/// How far the archive has been parsed, as a fraction (0 while it reports no file count)
fn parse_fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

/// GTex hashes worth looking up: non-blank and lowercased, since maclarian matches them against
/// the page file name case-sensitively.
fn vt_hashes(asset: &VisualAsset) -> Vec<String> {
    asset
        .virtual_textures
        .iter()
        .map(|vt| vt.gtex_hash.trim().to_lowercase())
        .filter(|hash| !hash.is_empty())
        .collect()
}

/// Every visual GUID in both ascending orders the list can be sorted by: by (name, id) and by id.
/// Both are built once, so a sort change only picks a different cached sequence.
fn sorted_visual_ids(db: &MergedDatabase) -> (Vec<String>, Vec<String>) {
    let mut rows: Vec<(&str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| (visual.name.as_str(), visual.id.as_str()))
        .collect();
    rows.sort_unstable();
    let by_name: Vec<String> = rows.into_iter().map(|(_, id)| id.to_string()).collect();

    let mut ids: Vec<&str> = db.visuals_by_id.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let by_id: Vec<String> = ids.into_iter().map(str::to_string).collect();

    (by_name, by_id)
}

/// Auto-detect the BG3 Data directory (default Steam install location)
#[tauri::command]
#[specta::specta]
pub fn detect_game_path(state: State<'_, SharedState>) -> Result<Option<String>, AppError> {
    let mut st = lock(&state)?;

    if !GameDataResolver::is_available() {
        return Ok(None);
    }

    match GameDataResolver::auto_detect() {
        Ok(resolver) => {
            let path = resolver.game_data_path().to_path_buf();
            st.game_path = Some(path.clone());
            st.resolver = Some(Arc::new(resolver));
            st.reset_index();
            Ok(Some(path.display().to_string()))
        }
        Err(err) => {
            eprintln!("[maclarian] auto-detect failed: {err}");
            Ok(None)
        }
    }
}

/// Read the BG3 Data directory currently recorded by the backend
/// (used to restore the UI state after switching pages)
#[tauri::command]
#[specta::specta]
pub fn get_game_path(state: State<'_, SharedState>) -> Result<Option<String>, AppError> {
    let st = lock(&state)?;
    Ok(st.game_path.as_ref().map(|p| p.display().to_string()))
}

/// Manually set the BG3 Data directory (must contain Shared.pak)
#[tauri::command]
#[specta::specta]
pub fn set_game_path(state: State<'_, SharedState>, path: String) -> Result<String, AppError> {
    let dir = PathBuf::from(&path);

    if !dir.exists() {
        return Err(AppError::DirectoryMissing { path });
    }
    if !dir.join("Shared.pak").exists() {
        return Err(AppError::SharedPakMissing { path });
    }

    let resolver = GameDataResolver::new(&dir).map_err(|err| AppError::ResolverInit {
        detail: err.to_string(),
    })?;

    let mut st = lock(&state)?;
    st.game_path = Some(dir);
    st.resolver = Some(Arc::new(resolver));
    st.reset_index();

    Ok(path)
}

/// Build the _merged resource database; progress is pushed through a Channel
#[tauri::command]
#[specta::specta]
pub async fn build_database(
    app: AppHandle,
    on_progress: Channel<BuildProgress>,
) -> Result<DatabaseStats, AppError> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<DatabaseStats, AppError> {
        // Take what the build needs, then release the lock: parsing Shared.pak runs for minutes.
        let (resolver, game_path) = {
            let st = lock(&state)?;
            (
                st.resolver.clone().ok_or(AppError::NotConfigured)?,
                st.game_path.clone().ok_or(AppError::NotConfigured)?,
            )
        };

        let channel = on_progress.clone();
        channel.send(BuildProgress { percent: 0.0 }).ok();

        let pak_path = game_path.join("Shared.pak");
        let mut db = MergedDatabase::new(game_path.display().to_string());
        let parsed =
            resolver.parse_pak_with_progress(&pak_path, &mut db, move |current, total, _| {
                let _ = channel.send(BuildProgress {
                    percent: parse_fraction(current, total),
                });
            });

        match parsed {
            Ok(()) if db.stats().visual_count > 0 => {}
            Ok(()) => db = resolver.database().clone(),
            Err(err) => {
                eprintln!("[maclarian] per-file parse failed, falling back to lazy build: {err}");
                db = resolver.database().clone();
            }
        }

        // Materials and textures only become reachable from their visuals once the whole database
        // has been parsed.
        db.resolve_references();

        // Read out while the database is here: maclarian exposes a material's name no other way.
        // The virtual texture parameter names are read per detail view instead.
        let materials = extract_materials(&db);

        let stats = db.stats();
        let (visual_ids, visual_ids_by_id) = sorted_visual_ids(&db);
        let visual_count = visual_ids.len();

        // A build that raced with a directory switch belongs to the old directory and is dropped.
        {
            let mut st = lock(&state)?;
            if st.game_path.as_deref() != Some(game_path.as_path()) {
                return Err(AppError::GameDirectoryChanged);
            }
            st.visual_ids = visual_ids;
            st.visual_ids_by_id = visual_ids_by_id;
            st.materials = materials;
            st.merged_db = Some(db);
        }

        on_progress.send(BuildProgress { percent: 1.0 }).ok();

        Ok(DatabaseStats {
            // One entry per visual GUID, so the dashboard always matches the browse list
            visual_count,
            material_count: stats.material_count,
            texture_count: stats.texture_count,
            virtual_texture_count: stats.virtual_texture_count,
        })
    })
    .await
    .map_err(|err| AppError::TaskTerminated {
        task: "Build",
        detail: err.to_string(),
    })?
}

/// Make sure the materials of `visual_id` carry the parameter name of every virtual texture binding.
///
/// maclarian parses a binding down to its GUID, and a merged document is always `_merged.lsf`, so
/// the names are read from the material's `SourceFile` template — same names, same order. Names
/// stay in `materials` once read, so a second view reads nothing at all.
fn ensure_virtual_texture_parameters(state: &SharedState, visual_id: &str) -> Result<(), AppError> {
    let (pending, pool, game_path) = {
        let mut st = lock(state)?;

        let pending: Vec<(String, String)> = st
            .merged_db
            .as_ref()
            .and_then(|db| db.visuals_by_id.get(visual_id))
            .map(|asset| {
                asset
                    .material_ids
                    .iter()
                    .filter_map(|material_id| {
                        let material = st.materials.get(material_id)?;
                        let unresolved = material
                            .virtual_textures
                            .iter()
                            .any(|binding| binding.parameter_name.is_empty());
                        (!material.source_file.is_empty() && unresolved)
                            .then(|| (material_id.clone(), material.source_file.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Nothing to read: every binding of this asset already carries its name
        if pending.is_empty() {
            return Ok(());
        }

        let game_path = st.game_path.clone().ok_or(AppError::NotConfigured)?;
        (pending, st.pool()?, game_path)
    };

    // Read on its own: the pool lock is not taken while the state lock is held
    let parameters = {
        let mut archives = lock_pool(&pool)?;
        virtual_texture_params::read_parameters(&mut archives, &pending)
    };

    let mut st = lock(state)?;
    // A directory switch can land while the templates are read: the names then belong to nothing
    if st.game_path.as_deref() != Some(game_path.as_path()) {
        return Ok(());
    }
    fill_virtual_texture_parameters(&mut st.materials, &parameters);
    Ok(())
}

/// Current database statistics (None when not built yet)
#[tauri::command]
#[specta::specta]
pub fn db_stats(state: State<'_, SharedState>) -> Result<Option<DatabaseStats>, AppError> {
    let st = lock(&state)?;

    match st.merged_db.as_ref() {
        Some(db) => {
            let stats = db.stats();
            Ok(Some(DatabaseStats {
                // Same as build_database: the dashboard should reflect what the UI can actually browse
                visual_count: st.visual_ids.len(),
                material_count: stats.material_count,
                texture_count: stats.texture_count,
                virtual_texture_count: stats.virtual_texture_count,
            }))
        }
        None => Ok(None),
    }
}

/// App metadata for the About page (compile-time values, no state required)
#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Browse visual assets page by page; `keyword` matches the name or GUID, `sort` picks the cached
/// order.
#[tauri::command]
#[specta::specta]
pub fn list_visuals(
    state: State<'_, SharedState>,
    offset: u32,
    limit: u32,
    keyword: Option<String>,
    sort: Option<String>,
    descending: Option<bool>,
) -> Result<Page<VisualSummary>, AppError> {
    let st = lock(&state)?;
    let db = st.merged_db.as_ref().ok_or(AppError::NotBuilt)?;

    // "id" opts into the GUID order; anything else (including a missing value) keeps the name order
    let ids = match sort.as_deref() {
        Some("id") => &st.visual_ids_by_id,
        _ => &st.visual_ids,
    };

    let keyword = keyword
        .map(|kw| kw.trim().to_lowercase())
        .filter(|kw| !kw.is_empty());

    let mut matched: Vec<&String> = ids
        .iter()
        .filter(|id| {
            keyword.as_deref().is_none_or(|kw| {
                // The cached order only holds GUIDs, so the name to match against comes from the
                // DB. GUIDs are ASCII, so folding them stays a cheap ASCII-lowercase compare.
                id.as_str().to_ascii_lowercase().contains(kw)
                    || db
                        .visuals_by_id
                        .get(*id)
                        .is_some_and(|visual| visual.name.to_lowercase().contains(kw))
            })
        })
        .collect();

    // The cached sequences are ascending and a page is sliced out of the matches, so a descending
    // request flips the whole match set rather than the page
    if descending.unwrap_or(false) {
        matched.reverse();
    }

    let total = matched.len();
    let start = (offset as usize).min(total);
    let end = (start + limit as usize).min(total);
    let items: Vec<VisualSummary> = matched[start..end]
        .iter()
        .filter_map(|id| db.visuals_by_id.get(*id))
        .map(VisualSummary::from)
        .collect();

    Ok(Page {
        items,
        total,
        offset: start,
    })
}

/// Query the detail of a single visual asset by its GUID (names are not unique).
///
/// Reading a page file size is the only archive work here, and it is off the main thread: the GTS
/// that carries it is one file read, not a scan.
#[tauri::command]
#[specta::specta]
pub async fn get_visual(app: AppHandle, id: String) -> Result<Option<VisualAssetDetail>, AppError> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<VisualAssetDetail>, AppError> {
        // The chips show the parameter each virtual texture fills; read the names first.
        ensure_virtual_texture_parameters(&state, &id)?;

        let (mut detail, matches, pool, page_file_sizes) = {
            let mut st = lock(&state)?;
            // Owned: the GTex lookup below needs `&mut` state.
            let asset = match st
                .merged_db
                .as_ref()
                .and_then(|db| db.visuals_by_id.get(&id))
            {
                Some(asset) => asset.clone(),
                None => return Ok(None),
            };

            let hashes = vt_hashes(&asset);
            let matches = if hashes.is_empty() {
                Vec::new()
            } else {
                st.vt_matches(&hashes)
            };
            let detail = VisualAssetDetail::new(&asset, &matches, &st.materials);

            // The size cache travels with the detail; the block below avoids holding the state lock
            // while the archives are read. `pool()` takes `&mut` as well, so it runs first
            let page_file_sizes = st.page_file_sizes.clone();
            (detail, matches, st.pool().ok(), page_file_sizes)
        };

        // Page file sizes need the archives; `page_file_size` reads each GTS once
        if let Some(pool) = pool {
            match lock_pool(&pool) {
                Ok(mut archives) => {
                    for vt in &mut detail.virtual_textures {
                        let size = match_for_hash(&matches, &vt.hash).and_then(|matched| {
                            virtual_textures::page_file_size(
                                &mut archives,
                                &page_file_sizes,
                                matched,
                                &vt.hash,
                            )
                        });
                        vt.set_size(size);
                    }
                }
                // A size is decoration: an unavailable pool leaves the rows without one
                Err(err) => eprintln!("[maclarian] page file sizes unavailable: {err}"),
            }
        }

        Ok(Some(detail))
    })
    .await
    .map_err(|err| AppError::TaskTerminated {
        task: "Detail",
        detail: err.to_string(),
    })?
}

/// Read the GR2 mesh of a visual asset and convert it to GLB for the three.js preview (geometry
/// only). Everything stays in memory, and the conversion runs in `spawn_blocking`.
#[tauri::command]
#[specta::specta]
pub async fn get_visual_preview(app: AppHandle, path: String) -> Result<ModelPreview, AppError> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<ModelPreview, AppError> {
        // The pool is shared with every other command; the state lock is released before any read.
        let pool = {
            let mut st = lock(&state)?;
            st.pool()?
        };

        // maclarian compares PAK entries with an exact `==`, which never matches Windows
        // separators; `Archives` normalizes instead.
        let gr2_bytes = lock_pool(&pool)?
            .read_from(Pak::Models, &path)
            .map_err(|err| {
                eprintln!("[maclarian] find gr2 {path} failed: {err}");
                err
            })?;

        let glb = convert_gr2_bytes_to_glb(&gr2_bytes).map_err(|err| {
            eprintln!("[maclarian] gr2 -> glb failed for {path}: {err}");
            // Pass through unchanged: the frontend maps maclarian's "No meshes found" message to
            // dedicated copy
            AppError::Message(err.to_string())
        })?;

        Ok(ModelPreview {
            base64: base64::engine::general_purpose::STANDARD.encode(&glb),
        })
    })
    .await
    .map_err(|err| AppError::TaskTerminated {
        task: "Preview",
        detail: err.to_string(),
    })?
}

/// Export a single visual asset: GR2 → GLB (or raw) + textures + virtual textures + asset.json.
///
/// Runs in `spawn_blocking`; the state lock is released once the needed data has been fetched.
#[tauri::command]
#[specta::specta]
pub async fn export_visual_asset(
    app: AppHandle,
    id: String,
    dest_dir: String,
    options: ExportOptions,
    on_progress: Channel<ExportProgress>,
) -> Result<ExportResult, AppError> {
    let state = app.state::<SharedState>().inner().clone();
    let dest_root = PathBuf::from(&dest_dir);

    if !dest_root.exists() {
        return Err(AppError::ExportDirectoryMissing { path: dest_dir });
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<ExportResult, AppError> {
        // The manifest states the parameter of every virtual texture binding, so read the names
        // first.
        ensure_virtual_texture_parameters(&state, &id)?;

        let (pool, asset, materials, vt_matches) = {
            let mut st = lock(&state)?;
            let asset = st
                .merged_db
                .as_ref()
                .and_then(|db| db.visuals_by_id.get(&id))
                .cloned()
                .ok_or(AppError::NotBuilt)?;
            // Page files are looked up only when virtual textures are part of the export
            let vt_matches = if options.texture_format.is_export() {
                st.vt_matches(&vt_hashes(&asset))
            } else {
                Vec::new()
            };
            // Cut out of the cache while the state is held: the export does not get the whole cache
            let materials = materials_of(&asset.material_ids, &st.materials);
            // Shared with the previews: the archives this export needs are usually open already
            (st.pool()?, asset, materials, vt_matches)
        };

        run_export(
            &asset,
            &materials,
            &pool,
            &dest_root,
            &options,
            &vt_matches,
            &|progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
    .await
    .map_err(|err| AppError::TaskTerminated {
        task: "Export",
        detail: err.to_string(),
    })?
}

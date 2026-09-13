use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GameDataResolver, MergedDatabase, VisualAsset};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::export::run_export;
use crate::archives::lock_pool;
use crate::models::{
    AppInfo, BuildProgress, DatabaseStats, ExportOptions, ExportProgress, ExportResult, ModelPreview,
    Page, VisualAssetDetail, VisualSummary,
};
use crate::state::AppState;

pub type SharedState = Arc<Mutex<AppState>>;

/// Lock the shared state; the only failure mode is a poisoned mutex, reported as a plain string
fn lock(state: &SharedState) -> Result<MutexGuard<'_, AppState>, String> {
    state.lock().map_err(|e| format!("State lock unavailable: {e}"))
}

const NOT_CONFIGURED: &str = "BG3 Data directory is not set. Please use auto-detect or select a directory first.";
const NOT_BUILT: &str = "Resource database has not been built. Please go to the Database page and build it first.";

/// How far the archive has been parsed, as a fraction (0 while it reports no file count)
fn parse_fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

/// GTex hashes worth looking up among a visual's virtual textures. Blank hashes are dropped: they
/// cannot match a page file, and looking them up would only re-list the archive. Hashing is
/// lowercased because maclarian matches it against the page file name case-sensitively, while the
/// database does not promise a case.
fn vt_hashes(asset: &VisualAsset) -> Vec<String> {
    asset
        .virtual_textures
        .iter()
        .map(|vt| vt.gtex_hash.trim().to_lowercase())
        .filter(|hash| !hash.is_empty())
        .collect()
}

/// Every visual GUID in both ascending orders the list can be sorted by: by (name, id) — the
/// default — and by id. Names are only a sort key: they are not unique, so using them as the
/// identity would collapse same-named visuals into one list row. Sorting by name first keeps those
/// duplicates adjacent while the id tiebreak keeps pagination deterministic.
///
/// Both orders are built here, once, so a sort change only picks a different cached sequence
/// instead of re-sorting every id on each page request.
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
pub fn detect_game_path(state: State<'_, SharedState>) -> Result<Option<String>, String> {
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
pub fn get_game_path(state: State<'_, SharedState>) -> Result<Option<String>, String> {
    let st = lock(&state)?;
    Ok(st.game_path.as_ref().map(|p| p.display().to_string()))
}

/// Manually set the BG3 Data directory (must contain Shared.pak)
#[tauri::command]
pub fn set_game_path(state: State<'_, SharedState>, path: String) -> Result<String, String> {
    let dir = PathBuf::from(&path);

    if !dir.exists() {
        return Err(format!("Directory does not exist: {path}"));
    }
    if !dir.join("Shared.pak").exists() {
        return Err(format!(
            "Shared.pak not found in this directory. Please select the BG3 Data directory: {path}"
        ));
    }

    let resolver = GameDataResolver::new(&dir)
        .map_err(|err| format!("Failed to initialize resource parser: {err}"))?;

    let mut st = lock(&state)?;
    st.game_path = Some(dir);
    st.resolver = Some(Arc::new(resolver));
    st.reset_index();

    Ok(path)
}

/// Build the _merged resource database; progress is pushed through a Channel
#[tauri::command]
pub async fn build_database(
    app: AppHandle,
    on_progress: Channel<BuildProgress>,
) -> Result<DatabaseStats, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<DatabaseStats, String> {
        // Take what the build needs, then release the lock immediately: parsing Shared.pak runs for
        // minutes, and every other command needs that lock to answer.
        let (resolver, game_path) = {
            let st = lock(&state)?;
            (
                st.resolver
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
                st.game_path
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
            )
        };

        let channel = on_progress.clone();
        channel.send(BuildProgress { percent: 0.0 }).ok();

        let pak_path = game_path.join("Shared.pak");
        let mut db = MergedDatabase::new(game_path.display().to_string());
        let parsed = resolver.parse_pak_with_progress(&pak_path, &mut db, move |current, total, _| {
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

        let stats = db.stats();
        let (visual_ids, visual_ids_by_id) = sorted_visual_ids(&db);
        let visual_count = visual_ids.len();

        // The lock is taken again only to publish the result. A build that raced with a directory
        // switch belongs to the directory that is no longer current, so it is dropped instead.
        {
            let mut st = lock(&state)?;
            if st.game_path.as_deref() != Some(game_path.as_path()) {
                return Err(
                    "Game data directory changed while building; the result was discarded."
                        .to_string(),
                );
            }
            st.visual_ids = visual_ids;
            st.visual_ids_by_id = visual_ids_by_id;
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
        .map_err(|err| format!("Build task terminated unexpectedly: {err}"))?
}

/// Current database statistics (None when not built yet)
#[tauri::command]
pub fn db_stats(state: State<'_, SharedState>) -> Result<Option<DatabaseStats>, String> {
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
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Browse visual assets page by page. `keyword` is an optional filter matched against the asset
/// name or its GUID (so a pasted ID finds its row); `sort` picks the column and `descending` the
/// direction, both resolved against the orders cached when the database was built.
#[tauri::command]
pub fn list_visuals(
    state: State<'_, SharedState>,
    offset: usize,
    limit: usize,
    keyword: Option<String>,
    sort: Option<String>,
    descending: Option<bool>,
) -> Result<Page<VisualSummary>, String> {
    let st = lock(&state)?;
    let db = st.merged_db.as_ref().ok_or_else(|| NOT_BUILT.to_string())?;

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
        .filter(|id| match keyword.as_deref() {
            None => true,
            Some(kw) => {
                // The cached order only holds GUIDs, so the name to match against comes from the
                // DB. GUIDs are ASCII, so folding them stays a cheap ASCII-lowercase compare.
                id.as_str().to_ascii_lowercase().contains(kw)
                    || db
                        .visuals_by_id
                        .get(*id)
                        .is_some_and(|visual| visual.name.to_lowercase().contains(kw))
            }
        })
        .collect();

    // The cached sequences are ascending and a page is sliced out of the matches, so a descending
    // request flips the whole match set rather than the page
    if descending.unwrap_or(false) {
        matched.reverse();
    }

    let total = matched.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
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
/// The archives holding the mesh and each texture are resolved here rather than at build time:
/// which PAK contains a file can only be answered by consulting the archives, and that scan is
/// heavy enough to belong off the main thread (see `Archives::locate_many`). Virtual textures take
/// the other route: maclarian reports page files as `GtpMatch` values that carry their own path
/// and archive, so nothing has to be scanned for them.
#[tauri::command]
pub async fn get_visual(
    app: AppHandle,
    id: String,
) -> Result<Option<VisualAssetDetail>, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<VisualAssetDetail>, String> {
        let (mut detail, pool) = {
            let mut st = lock(&state)?;
            // Owned rather than borrowed: the GTex lookup below needs `&mut` state (it hands the
            // database to a resolver and takes it back), which no borrow of the database outlives.
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
            let detail = VisualAssetDetail::new(&asset, &matches);

            // `pool()` takes `&mut`, so it runs after the detail is owned; the lock is dropped with
            // this block, leaving the archives to be scanned without holding the state
            (detail, st.pool().ok())
        };

        // The mesh and every DDS go into one batch: `locate_many` walks the cached file tables a
        // single time and decompresses nothing, so the texture paths ride along on the scan the mesh
        // already needed. Textures are not confined to `Textures.pak` (there is also
        // `Gustav_Textures.pak`, `LowTex.pak`, `Icons.pak`), which is why each one is located instead
        // of assumed
        let located = match pool {
            Some(pool) => {
                let mut targets = Vec::with_capacity(detail.textures.len() + 1);
                targets.push(detail.path.clone());
                targets.extend(detail.textures.iter().map(|tex| tex.path.clone()));
                lock_pool(&pool)
                    .map(|mut archives| archives.locate_many(&targets))
                    .unwrap_or_default()
            }
            None => HashMap::new(),
        };

        // Archive names are decoration: an unresolved file just renders without one
        detail.mesh_pak = located.get(&detail.path).cloned().unwrap_or_default();
        for tex in &mut detail.textures {
            tex.source = located.get(&tex.path).cloned().unwrap_or_default();
        }

        Ok(Some(detail))
    })
        .await
        .map_err(|err| format!("Detail task terminated unexpectedly: {err}"))?
}

/// Read the GR2 mesh of a visual asset and convert it to GLB for the frontend three.js preview
/// (geometry only, no textures). Everything stays in memory: Shared.pak bytes → GLB → Base64, with
/// no intermediate files.
/// The conversion (BitKnit decompression + parsing) can take several seconds, so it runs inside
/// spawn_blocking to avoid freezing the UI
#[tauri::command]
pub async fn get_visual_preview(
    app: AppHandle,
    path: String,
) -> Result<ModelPreview, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<ModelPreview, String> {
        // The pool is shared with every other command, so a preview no longer re-opens the archives.
        // Only the handle is taken here: the state lock is released before anything is read.
        let pool = {
            let mut st = lock(&state)?;
            st.pool()?
        };

        // maclarian's own reader compares PAK entries with an exact `==` on the raw path, which
        // never matches on Windows (`\` vs `/`); the `Archives` pool normalizes separators and casing
        // instead.
        let gr2_bytes = lock_pool(&pool)?
            .read(&path, Some("Models.pak"))
            .map_err(|err| {
                eprintln!("[maclarian] find gr2 {path} failed: {err}");
                err
            })?;

        let glb = convert_gr2_bytes_to_glb(&gr2_bytes).map_err(|err| {
            eprintln!("[maclarian] gr2 -> glb failed for {path}: {err}");
            // Pass through unchanged: for a mesh-less GR2 maclarian returns
            // "No meshes found in GR2 file (...)", which the frontend maps to dedicated copy;
            // anything else falls through to the generic failure message
            err.to_string()
        })?;

        Ok(ModelPreview {
            base64: base64::engine::general_purpose::STANDARD.encode(&glb),
        })
    })
        .await
        .map_err(|err| format!("Preview task terminated unexpectedly: {err}"))?
}

/// Export a single visual asset: GR2 → GLB (optionally embedded textures) + textures +
/// virtual textures + asset.json.
///
/// Reading PAKs, converting and writing to disk are all slow, so this runs inside spawn_blocking;
/// the state lock is released as soon as the needed data has been fetched.
#[tauri::command]
pub async fn export_visual_asset(
    app: AppHandle,
    id: String,
    dest_dir: String,
    options: ExportOptions,
    on_progress: Channel<ExportProgress>,
) -> Result<ExportResult, String> {
    let state = app.state::<SharedState>().inner().clone();
    let dest_root = PathBuf::from(&dest_dir);

    if !dest_root.exists() {
        return Err(format!("Export directory does not exist: {dest_dir}"));
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<ExportResult, String> {
        let (pool, asset, vt_matches) = {
            let mut st = lock(&state)?;
            let asset = st
                .merged_db
                .as_ref()
                .and_then(|db| db.visuals_by_id.get(&id))
                .cloned()
                .ok_or_else(|| NOT_BUILT.to_string())?;
            // Page files are looked up only when virtual textures are part of the export; with an
            // empty list the manifest rows simply carry no path and no archive
            let vt_matches = if options.texture_format.is_export() {
                st.vt_matches(&vt_hashes(&asset))
            } else {
                Vec::new()
            };
            // Shared with the previews: the archives this export needs are usually open already
            (st.pool()?, asset, vt_matches)
        };

        run_export(
            &asset,
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
        .map_err(|err| format!("Export task terminated unexpectedly: {err}"))?
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GameDataResolver, MergedDatabase};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::export::{lock_pool, run_export};
use crate::models::{
    AppInfo, AssetSource, BuildProgress, DatabaseStats, ExportOptions, ExportProgress,
    ExportResult, ModelPreview, Page, VisualAssetDetail, VisualSummary,
};
use crate::mods;
use crate::state::AppState;

pub type SharedState = Arc<Mutex<AppState>>;

/// Lock the shared state; the only failure mode is a poisoned mutex, reported as a plain string
fn lock(state: &SharedState) -> Result<MutexGuard<'_, AppState>, String> {
    state.lock().map_err(|e| format!("State lock unavailable: {e}"))
}

const NOT_CONFIGURED: &str = "BG3 Data directory is not set. Please use auto-detect or select a directory first.";
const NOT_BUILT: &str = "Resource database has not been built. Please go to the Database page and build it first.";

/// Share of the build progress bar spent on the base game archives. Installed mods share the rest,
/// so the reported percentage keeps rising across both phases instead of jumping backwards.
const BASE_PROGRESS_SHARE: f32 = 0.9;
const MOD_PROGRESS_SHARE: f32 = 1.0 - BASE_PROGRESS_SHARE;

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

        // Mod archives live outside the game directory, so the PAK pool cannot find them on its own:
        // the list is resolved once here and registered before the pool is (re)created.
        let mod_paks = mods::list_mod_paks();
        let mod_count = mod_paks.len();

        {
            let mut st = lock(&state)?;
            // The pool caches the per-path mod index, so a changed mod list invalidates it
            if st.mod_paks != mod_paks {
                st.mod_paks = mod_paks.clone();
                st.packages = None;
            }
        }

        let channel = on_progress.clone();
        channel
            .send(BuildProgress {
                current: 0,
                total: 0,
                current_file: Some("Shared.pak".to_string()),
                percent: 0.0,
            })
            .ok();

        // Without mods the base game archive owns the whole bar; with mods it stops at 90%
        let base_share = if mod_count > 0 {
            BASE_PROGRESS_SHARE
        } else {
            1.0
        };

        let pak_path = game_path.join("Shared.pak");
        let mut db = MergedDatabase::new(game_path.display().to_string());
        let parsed = resolver.parse_pak_with_progress(&pak_path, &mut db, move |current, total, file| {
            let fraction = if total == 0 {
                0.0
            } else {
                current as f32 / total as f32
            };
            let _ = channel.send(BuildProgress {
                current,
                total,
                current_file: Some(file.to_string()),
                percent: base_share * fraction,
            });
        });

        // The fallback is decided on the base game parse alone: otherwise a mod that does ship
        // assets would mask a base game archive that failed to parse.
        match parsed {
            Ok(()) if db.stats().visual_count > 0 => {}
            Ok(()) => db = resolver.database().clone(),
            Err(err) => {
                eprintln!("[maclarian] per-file parse failed, falling back to lazy build: {err}");
                db = resolver.database().clone();
            }
        }

        // Mods are parsed on top of the base game database. maclarian merges into the target
        // database with the later parse winning, which is the same precedence the game applies, and
        // it is the only merge entry point: `merge_databases` and the material table are crate-private.
        for (index, pak) in mod_paks.iter().enumerate() {
            let channel = on_progress.clone();
            let slot_start = BASE_PROGRESS_SHARE + MOD_PROGRESS_SHARE * index as f32 / mod_count as f32;
            let slot_span = MOD_PROGRESS_SHARE / mod_count as f32;
            let parsed = resolver.parse_pak_with_progress(pak, &mut db, move |current, total, file| {
                let fraction = if total == 0 {
                    0.0
                } else {
                    current as f32 / total as f32
                };
                let _ = channel.send(BuildProgress {
                    current,
                    total,
                    current_file: Some(file.to_string()),
                    percent: slot_start + slot_span * fraction,
                });
            });

            // One broken archive must not abort the build: the other mods are still worth indexing
            if let Err(err) = parsed {
                eprintln!(
                    "[maclarian] mod archive {} failed to parse: {err}",
                    pak.display()
                );
            }
        }

        // Resolved once for the whole database: mod materials/textures only become reachable from
        // their visuals after every archive has been merged in.
        db.resolve_references();

        let stats = db.stats();
        let mut visual_names: Vec<String> = db.visual_names().map(|n| n.to_string()).collect();
        visual_names.sort();
        let visual_count = visual_names.len();

        // The PAK pool is the only place that knows which archive family a path resolves to, so
        // every visual name is checked against the mod index here and the result cached on the
        // state. The browse page uses this map to filter the list without touching the pool again.
        let visual_sources = {
            let pool = lock(&state).ok().and_then(|mut st| st.pool().ok());
            let pool_guard = pool.as_ref().and_then(|arc| lock_pool(arc).ok());
            let mut sources: HashMap<String, AssetSource> =
                HashMap::with_capacity(visual_names.len());
            for name in &visual_names {
                let origin = match (&pool_guard, db.get_by_visual_name(name)) {
                    (Some(pool), Some(asset)) => {
                        if pool.mod_pak_for(&asset.gr2_path).is_some()
                            || asset
                                .textures
                                .iter()
                                .any(|tex| pool.mod_pak_for(&tex.dds_path).is_some())
                        {
                            AssetSource::Mod
                        } else {
                            AssetSource::Base
                        }
                    }
                    _ => AssetSource::Base,
                };
                sources.insert(name.clone(), origin);
            }
            sources
        };

        // The dashboard reads these counts on every render; computing them once here keeps the
        // hot path off the PAK pool.
        let base_visual_count = visual_sources
            .values()
            .filter(|origin| **origin == AssetSource::Base)
            .count();
        let mod_visual_count = visual_sources
            .values()
            .filter(|origin| **origin == AssetSource::Mod)
            .count();

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
            st.visual_names = visual_names;
            st.visual_sources = visual_sources;
            st.merged_db = Some(db);
        }

        on_progress
            .send(BuildProgress {
                current: 1,
                total: 1,
                current_file: None,
                percent: 1.0,
            })
            .ok();

        Ok(DatabaseStats {
            // The UI browses assets by visual name, and visuals_by_name collapses duplicates.
            // Count unique names so the dashboard matches the browse list.
            visual_count,
            material_count: stats.material_count,
            texture_count: stats.texture_count,
            virtual_texture_count: stats.virtual_texture_count,
            mod_count,
            base_visual_count,
            mod_visual_count,
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
            let base_visual_count = st
                .visual_sources
                .values()
                .filter(|origin| **origin == AssetSource::Base)
                .count();
            let mod_visual_count = st
                .visual_sources
                .values()
                .filter(|origin| **origin == AssetSource::Mod)
                .count();
            Ok(Some(DatabaseStats {
                // Same as build_database: the dashboard should reflect what the UI can actually browse.
                visual_count: db.visual_names().count(),
                material_count: stats.material_count,
                texture_count: stats.texture_count,
                virtual_texture_count: stats.virtual_texture_count,
                mod_count: st.mod_paks.len(),
                base_visual_count,
                mod_visual_count,
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

/// Browse visual assets page by page (keyword is an optional filter, kept for later search reuse)
#[tauri::command]
pub fn list_visuals(
    state: State<'_, SharedState>,
    offset: usize,
    limit: usize,
    keyword: Option<String>,
    source: Option<String>,
) -> Result<Page<VisualSummary>, String> {
    let st = lock(&state)?;
    let db = st.merged_db.as_ref().ok_or_else(|| NOT_BUILT.to_string())?;

    // The source filter is keyed off the per-name map built during `build_database`, so a request
    // that asks for "mod" never has to touch the PAK pool.
    let wanted: Option<AssetSource> = match source.as_deref() {
        Some("base") => Some(AssetSource::Base),
        Some("mod") => Some(AssetSource::Mod),
        _ => None,
    };

    let matched: Vec<&String> = match keyword {
        Some(kw) if !kw.trim().is_empty() => {
            let kw = kw.to_lowercase();
            st.visual_names
                .iter()
                .filter(|name| {
                    name.to_lowercase().contains(&kw)
                        && wanted
                            .map(|origin| {
                                st.visual_sources
                                    .get(*name)
                                    .copied()
                                    .unwrap_or(AssetSource::Base)
                                    == origin
                            })
                            .unwrap_or(true)
                })
                .collect()
        }
        _ => match wanted {
            Some(origin) => st
                .visual_names
                .iter()
                .filter(|name| {
                    st.visual_sources
                        .get(*name)
                        .copied()
                        .unwrap_or(AssetSource::Base)
                        == origin
                })
                .collect(),
            None => st.visual_names.iter().collect(),
        },
    };

    let total = matched.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let items: Vec<VisualSummary> = matched[start..end]
        .iter()
        .filter_map(|name| db.get_by_visual_name(name).map(|asset| (name, asset)))
        .map(|(name, asset)| {
            let mut summary = VisualSummary::from(asset);
            summary.origin = st
                .visual_sources
                .get(*name)
                .copied()
                .unwrap_or(AssetSource::Base);
            summary
        })
        .collect();

    Ok(Page {
        items,
        total,
        offset: start,
    })
}

/// Query the detail of a single visual asset
#[tauri::command]
pub fn get_visual(
    state: State<'_, SharedState>,
    name: String,
) -> Result<Option<VisualAssetDetail>, String> {
    let st = lock(&state)?;
    Ok(st
        .merged_db
        .as_ref()
        .and_then(|db| db.get_by_visual_name(&name))
        .map(VisualAssetDetail::from))
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
        // never matches on Windows (`\` vs `/`); PakPool normalizes separators and casing instead.
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
    name: String,
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
        let (pool, asset, gtp_index) = {
            let mut st = lock(&state)?;
            let asset = st
                .merged_db
                .as_ref()
                .and_then(|db| db.get_by_visual_name(&name))
                .cloned()
                .ok_or_else(|| NOT_BUILT.to_string())?;
            // The GTP index is only needed when virtual textures are exported separately
            let gtp_index = if options.texture_format.is_export() {
                st.gtp_index()
            } else {
                Vec::new()
            };
            // Shared with the previews: the archives this export needs are usually open already
            (st.pool()?, asset, gtp_index)
        };

        run_export(
            &asset,
            &pool,
            &dest_root,
            &options,
            &gtp_index,
            &|progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
        .await
        .map_err(|err| format!("Export task terminated unexpectedly: {err}"))?
}

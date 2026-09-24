use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GameDataResolver, MergedDatabase, VisualAsset};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::archives::{lock_pool, Pak};
use crate::cache::{self, Loaded};
use crate::export::{run_export, ExportContext};
use crate::models::{
    match_for_hash, materials_of, AppInfo, BuildProgress, CacheStatus, DatabaseStats, ExportOptions,
    ExportProgress, ExportResult, ModelPreview, Page, VisualAssetDetail, VisualSummary,
};
use crate::mods;
use crate::state::{extract_materials, fill_virtual_texture_parameters, AppState, ModSources};
use crate::virtual_texture_params;
use crate::virtual_textures;

pub type SharedState = Arc<Mutex<AppState>>;

/// Lock the shared state; the only failure mode is a poisoned mutex, reported as a plain string
fn lock(state: &SharedState) -> Result<MutexGuard<'_, AppState>, String> {
    state
        .lock()
        .map_err(|e| format!("State lock unavailable: {e}"))
}

const NOT_CONFIGURED: &str =
    "BG3 Data directory is not set. Please use auto-detect or select a directory first.";
const NOT_BUILT: &str =
    "Resource database has not been built. Please go to the Database page and build it first.";

/// Share of the build progress the mods are given, when there are any. The game data is one huge
/// archive and the mods are a handful of files, so the split is fixed rather than measured: it keeps
/// the bar from jumping back once the game data is done.
const MOD_WEIGHT: f32 = 0.2;

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

/// Every visual GUID in the ascending orders the list can be sorted by: by (name, id) — the default —
/// by id, and by (source, name, id). Names and sources are only sort keys: neither is unique, so
/// using them as the identity would collapse same-keyed visuals into one list row. Sorting by name
/// first keeps those duplicates adjacent while the id tiebreak keeps pagination deterministic. The
/// source order puts the base game first (its records carry no source) and then the mods by name.
///
/// All orders are built here, once, so a sort change only picks a different cached sequence instead
/// of re-sorting every id on each page request.
fn sorted_visual_ids(
    db: &MergedDatabase,
    sources: &ModSources,
) -> (Vec<String>, Vec<String>, Vec<String>) {
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

    let mut sourced: Vec<(&str, &str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| {
            (
                sources
                    .get(&visual.id)
                    .map(String::as_str)
                    .unwrap_or_default(),
                visual.name.as_str(),
                visual.id.as_str(),
            )
        })
        .collect();
    sourced.sort_unstable();
    let by_source: Vec<String> = sourced
        .into_iter()
        .map(|(_, _, id)| id.to_string())
        .collect();

    (by_name, by_id, by_source)
}

/// Auto-detect the BG3 Data directory (default Steam install location).
///
/// Off the main thread like every other command that touches the disk: the probe walks the default
/// install locations and, once a directory is found, the index a previous build left for it is read
/// back (see `restore_cached_database`).
#[tauri::command]
pub async fn detect_game_path(app: AppHandle) -> Result<Option<String>, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        if !GameDataResolver::is_available() {
            return Ok(None);
        }

        let (path, resolver) = match GameDataResolver::auto_detect() {
            Ok(resolver) => {
                let path = resolver.game_data_path().to_path_buf();
                (path, resolver)
            }
            Err(err) => {
                eprintln!("[maclarian] auto-detect failed: {err}");
                return Ok(None);
            }
        };

        {
            let mut st = lock(&state)?;
            st.game_path = Some(path.clone());
            st.resolver = Some(Arc::new(resolver));
            st.reset_index();
        }

        // The directory has just been (re)selected, which is where the index for it can come back
        restore_cached_database(&app, &state);

        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|err| format!("Auto-detect task terminated unexpectedly: {err}"))?
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
pub async fn set_game_path(app: AppHandle, path: String) -> Result<String, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
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

        // Held only around the switch: the read below does not want the state lock, and a build
        // running concurrently has to find the new directory rather than the old one
        {
            let mut st = lock(&state)?;
            st.game_path = Some(dir);
            st.resolver = Some(Arc::new(resolver));
            st.reset_index();
        }

        // The directory has just been (re)selected, which is where the index for it can come back
        restore_cached_database(&app, &state);

        Ok(path)
    })
    .await
    .map_err(|err| format!("Directory change terminated unexpectedly: {err}"))?
}

/// Fill the index for the current directory out of the file a previous build left for it.
///
/// Called right after a directory has been selected, where `reset_index` has just emptied the state:
/// a matching file turns a launch — or a switch back to a directory that was scanned before — into a
/// read instead of a scan. The file is read with the state lock released, and the lock is taken again
/// only to publish: twenty-odd MB take long enough that holding it would stall every other command.
///
/// Nothing here fails the caller. A file that cannot be read, or no longer matches the sources it was
/// built from, only means there is nothing to restore — a scan is still there to be clicked — and the
/// reason is reported through the state for the page to show (see `CacheStatus`).
fn restore_cached_database(app: &AppHandle, state: &SharedState) {
    let game_path = match lock(state) {
        Ok(st) => st.game_path.clone(),
        Err(err) => {
            eprintln!("[cache] {err}");
            return;
        }
    };
    let Some(game_path) = game_path else {
        return;
    };

    let (status, persisted) = match cache::load(app, &game_path) {
        Loaded::Cache(persisted) => (CacheStatus::loaded(persisted.built_at), Some(*persisted)),
        Loaded::Missing => (CacheStatus::idle(), None),
        Loaded::Stale { code, detail } => (CacheStatus::stale(code, detail), None),
    };

    let mut st = match lock(state) {
        Ok(st) => st,
        Err(err) => {
            eprintln!("[cache] {err}");
            return;
        }
    };
    // A directory switch can land while the file is read: what came out of it then belongs to
    // resources that are no longer current
    if st.game_path.as_deref() != Some(game_path.as_path()) {
        return;
    }

    if let Some(code) = status.code.as_deref() {
        eprintln!(
            "[cache] {} not restored ({code}): {}",
            game_path.display(),
            status.detail.as_deref().unwrap_or("no detail")
        );
    }

    if let Some(persisted) = persisted {
        // The three orders are derived rather than persisted: they are a sort of the ids that are in
        // the database anyway, and recomputing them here keeps them consistent with the source map
        let (visual_ids, visual_ids_by_id, visual_ids_by_source) =
            sorted_visual_ids(&persisted.database, &persisted.mod_sources);
        st.visual_ids = visual_ids;
        st.visual_ids_by_id = visual_ids_by_id;
        st.visual_ids_by_source = visual_ids_by_source;
        st.materials = persisted.materials;
        st.mod_sources = persisted.mod_sources;
        st.merged_db = Some(persisted.database);
    }

    st.cache_status = status;
}

/// Build the _merged resource database; progress is pushed through a Channel
#[tauri::command]
pub async fn build_database(
    app: AppHandle,
    on_progress: Channel<BuildProgress>,
) -> Result<DatabaseStats, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<DatabaseStats, String> {
        // Take what the build needs, then release the lock immediately: parsing the game's packs runs
        // for minutes, and every other command needs that lock to answer. The pool is taken here too,
        // so it is built from the mod directory this build belongs to.
        let (resolver, game_path, pool) = {
            let mut st = lock(&state)?;
            (
                st.resolver
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
                st.game_path
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
                st.pool()?,
            )
        };

        let channel = on_progress.clone();
        channel.send(BuildProgress { percent: 0.0 }).ok();

        // How many mods there are settles the shape of the progress bar before any of them is read
        let mod_count = lock_pool(&pool)
            .map(|archives| archives.mod_count())
            .unwrap_or(0);
        let mod_weight = if mod_count == 0 { 0.0 } else { MOD_WEIGHT };
        let game_weight = 1.0 - mod_weight;

        // The packs maclarian builds its own database from, in its own order: `Shared.pak` first, then
        // `GustavX.pak`. A directory without the expansion pak builds from `Shared.pak` alone, the
        // same way the library's own build does.
        let paks: Vec<PathBuf> = ["Shared.pak", "GustavX.pak"]
            .iter()
            .map(|name| game_path.join(name))
            .filter(|path| path.is_file())
            .collect();

        let mut db = MergedDatabase::new(game_path.display().to_string());
        // A second handle for the parse callbacks, which take their channel by value: the mods below
        // keep reporting through this one
        let parse_channel = channel.clone();
        // Both packs report through the game's single progress segment, and neither file count is
        // known before it is read, so the segment is split evenly. The pieces never overlap, so the
        // bar only ever moves forward.
        //
        // Whether every pak was read is tracked for the persisted database below: a build that came
        // out short must not be written to disk, where its stamp of the sources would look unchanged
        // on the next launch and keep the incomplete index alive.
        let mut complete = true;
        let share = game_weight / paks.len().max(1) as f32;
        for (index, pak_path) in paks.iter().enumerate() {
            let base = share * index as f32;
            let progress = parse_channel.clone();
            let parsed =
                resolver.parse_pak_with_progress(pak_path, &mut db, move |current, total, _| {
                    let _ = progress.send(BuildProgress {
                        percent: base + share * parse_fraction(current, total),
                    });
                });
            if let Err(err) = parsed {
                // A pak that fails to parse costs only the resources it holds: the build carries on
                // with the others, so a broken `GustavX.pak` never costs the game's own data
                eprintln!("[maclarian] {} failed to parse: {err}", pak_path.display());
                complete = false;
            }
        }
        if db.stats().visual_count == 0 {
            eprintln!("[maclarian] no visuals parsed from the game packs");
        }

        // Materials and textures only become reachable from their visuals once the whole database
        // has been parsed. This runs before the mods are merged, so it only ever sees game data:
        // mod visuals resolve their own references as they are merged (see `mods::merge_into`).
        db.resolve_references();

        // Read out while the database is here rather than on the first detail view: maclarian does
        // not expose a material's name any other way (see `extract_materials`), and a one-off cost
        // inside a build that already runs for minutes is not something a click should pay for.
        //
        // The parameter names of the virtual texture bindings are *not* read here either: they come
        // from the materials' own templates and are read per detail view (see
        // `ensure_virtual_texture_parameters`).
        let mut materials = extract_materials(&db);

        // Mods come last, over a database that already holds everything the game provides: a mod
        // overrides what it redefines, and its visuals routinely bind the game's own materials.
        let mut mod_sources = ModSources::new();
        if mod_count > 0 {
            let mut archives = lock_pool(&pool)?;
            for index in 0..mod_count {
                let assets = mods::read_mod(&mut archives, index);
                mods::merge_into(&mut db, assets, &mut materials, &mut mod_sources);

                let done = (index + 1) as f32 / mod_count as f32;
                channel
                    .send(BuildProgress {
                        percent: game_weight + mod_weight * done,
                    })
                    .ok();
            }
        }

        let (visual_ids, visual_ids_by_id, visual_ids_by_source) =
            sorted_visual_ids(&db, &mod_sources);
        let visual_count = visual_ids.len();

        // Read off the merged maps rather than `db.stats()`: maclarian keeps the material map to
        // itself, so the mod materials are only ever in the cache above
        let stats = DatabaseStats {
            // One entry per visual GUID, so the dashboard always matches the browse list
            visual_count,
            material_count: materials.len(),
            texture_count: db.textures.len(),
            virtual_texture_count: db.virtual_textures.len(),
        };

        // Persisted while the database is still ours to borrow: the launch after this one then reads
        // the index back instead of parsing the paks again. Written before it is published (a 20 MB
        // file must not be written under the state lock), and only for a build that read everything —
        // an index missing a pak's resources would otherwise outlive the failure it came from. A
        // failed write costs the next launch a scan and nothing else, so it does not fail the build.
        if complete && visual_count > 0 {
            if let Err(err) = cache::save(&app, &game_path, &db, &materials, &mod_sources) {
                eprintln!("[cache] database not persisted: {err}");
            }
        }

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
            st.visual_ids_by_source = visual_ids_by_source;
            st.materials = materials;
            st.mod_sources = mod_sources;
            st.merged_db = Some(db);
            // Whatever the disk had to say is superseded by an index built in this session
            st.cache_status = CacheStatus::idle();
        }

        on_progress.send(BuildProgress { percent: 1.0 }).ok();

        Ok(stats)
    })
    .await
    .map_err(|err| format!("Build task terminated unexpectedly: {err}"))?
}

/// Make sure the materials of `visual_id` carry the parameter name of every virtual texture binding.
///
/// maclarian parses a binding down to its GUID, so the name it fills (`virtualtexture`,
/// `overlayvirtualtexture`, …) has to be read from the LSF documents — and the document a material
/// itself lives in cannot be derived from that material, because a merged file is always named
/// `_merged.lsf`. The material's `SourceFile` template is a real file, and it declares the same names
/// in the same order, so only the templates of this one asset's materials are read: a couple of
/// archive reads instead of a walk over every `_merged.lsf` of `Shared.pak` (see
/// `virtual_texture_params`).
///
/// The names stay in `materials` once read, so an already-named material is skipped and a second view
/// of the same asset reads nothing at all.
fn ensure_virtual_texture_parameters(state: &SharedState, visual_id: &str) -> Result<(), String> {
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

        let game_path = st
            .game_path
            .clone()
            .ok_or_else(|| NOT_CONFIGURED.to_string())?;
        (pending, st.pool()?, game_path)
    };

    // Read on its own: the templates come out of the archives, and the pool lock is not taken while
    // the state lock is held
    let parameters = {
        let mut archives = lock_pool(&pool)?;
        virtual_texture_params::read_parameters(&mut archives, &pending)
    };

    let mut st = lock(state)?;
    // A directory switch can land while the templates are read: the names then belong to materials
    // that are already gone
    if st.game_path.as_deref() != Some(game_path.as_path()) {
        return Ok(());
    }
    fill_virtual_texture_parameters(&mut st.materials, &parameters);
    Ok(())
}

/// Current database statistics (None when not built yet)
#[tauri::command]
pub fn db_stats(state: State<'_, SharedState>) -> Result<Option<DatabaseStats>, String> {
    let st = lock(&state)?;

    match st.merged_db.as_ref() {
        Some(db) => {
            let stats = DatabaseStats {
                // Same as build_database: the dashboard should reflect what the UI can actually browse
                visual_count: st.visual_ids.len(),
                material_count: st.materials.len(),
                texture_count: db.textures.len(),
                virtual_texture_count: db.virtual_textures.len(),
            };
            Ok(Some(stats))
        }
        None => Ok(None),
    }
}

/// Where the index the statistics above describe came from: read back from disk, or built here.
///
/// Read after the game directory has been restored rather than before: until a directory is selected
/// there is nothing to report, and selecting one is exactly when the persisted file is looked for.
#[tauri::command]
pub fn cache_status(state: State<'_, SharedState>) -> Result<CacheStatus, String> {
    Ok(lock(&state)?.cache_status.clone())
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
/// direction, both resolved against the orders cached when the database was built. `origin` splits
/// the index in two: "mod" keeps only what a mod provides, "base" only the game's own resources,
/// and anything else (including a missing value) keeps both.
#[tauri::command]
pub fn list_visuals(
    state: State<'_, SharedState>,
    offset: usize,
    limit: usize,
    keyword: Option<String>,
    sort: Option<String>,
    descending: Option<bool>,
    origin: Option<String>,
) -> Result<Page<VisualSummary>, String> {
    let st = lock(&state)?;
    let db = st.merged_db.as_ref().ok_or_else(|| NOT_BUILT.to_string())?;

    // "id" opts into the GUID order and "source" into the mod-name order; anything else (including a
    // missing value) keeps the name order
    let ids = match sort.as_deref() {
        Some("id") => &st.visual_ids_by_id,
        Some("source") => &st.visual_ids_by_source,
        _ => &st.visual_ids,
    };

    let keyword = keyword
        .map(|kw| kw.trim().to_lowercase())
        .filter(|kw| !kw.is_empty());

    let mut matched: Vec<&String> = ids
        .iter()
        .filter(|id| {
            // Mod-provided ids are exactly the ones the source table lists, so membership in it is
            // the whole test; the game's own resources are never written there.
            let from_mod = st.mod_sources.contains_key(id.as_str());
            match origin.as_deref() {
                Some("mod") if !from_mod => return false,
                Some("base") if from_mod => return false,
                _ => {}
            }

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
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let items: Vec<VisualSummary> = matched[start..end]
        .iter()
        .filter_map(|id| db.visuals_by_id.get(*id))
        .map(|visual| VisualSummary::of(visual, &st.mod_sources))
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
pub async fn get_visual(app: AppHandle, id: String) -> Result<Option<VisualAssetDetail>, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<VisualAssetDetail>, String> {
        // The binding chips show the parameter each virtual texture fills. Those names are not in the
        // parsed database, so they are read off this asset's material templates first — and only
        // while some binding of this asset has no name yet (see
        // `ensure_virtual_texture_parameters`).
        ensure_virtual_texture_parameters(&state, &id)?;

        let (mut detail, matches, pool, mut page_file_sizes) = {
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
            let detail = VisualAssetDetail::new(&asset, &matches, &st.materials, &st.mod_sources);

            // The size cache travels with the detail and is put back at the end: it is only ever
            // touched here, and holding the state lock while the archives are read is what the
            // block below exists to avoid. `pool()` takes `&mut` as well, so it runs first
            let page_file_sizes = std::mem::take(&mut st.page_file_sizes);
            (detail, matches, st.pool().ok(), page_file_sizes)
        };

        // Page file sizes need the archives: a GTS is one file read from the virtual texture archive
        // (no scan), and each of them serves every page file of its tile set, so `page_file_size`
        // sees to reading one only once
        if let Some(pool) = pool {
            match lock_pool(&pool) {
                Ok(mut archives) => {
                    for vt in &mut detail.virtual_textures {
                        let size = match_for_hash(&matches, &vt.hash).and_then(|matched| {
                            virtual_textures::page_file_size(
                                &mut archives,
                                &mut page_file_sizes,
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

        // Back into the state, for the next detail view of this game directory
        lock(&state)?.page_file_sizes = page_file_sizes;

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
pub async fn get_visual_preview(app: AppHandle, path: String) -> Result<ModelPreview, String> {
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
            .read_from(Pak::Models, &path)
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
        // The manifest states the parameter of every virtual texture binding, both on the material
        // rows and on the virtual texture rows. Those names are not in the parsed database, so they
        // are read off this asset's material templates first — and only while some binding of this
        // asset has no name yet (see `ensure_virtual_texture_parameters`).
        ensure_virtual_texture_parameters(&state, &id)?;

        let (pool, asset, materials, sources, vt_matches) = {
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
            // Cut out of the cache while the state is held: the manifest names the materials of the
            // asset, and the cache they live in is not handed to the export
            let materials = materials_of(&asset.material_ids, &st.materials);
            // Taken whole: unlike the material cache it only holds the resources the mods provide,
            // and the manifest labels every row of the asset with it
            let sources = st.mod_sources.clone();
            // Shared with the previews: the archives this export needs are usually open already
            (st.pool()?, asset, materials, sources, vt_matches)
        };

        run_export(
            &ExportContext {
                asset: &asset,
                materials: &materials,
                sources: &sources,
                vt_matches: &vt_matches,
                pool: &pool,
            },
            &dest_root,
            &options,
            &|progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
    .await
    .map_err(|err| format!("Export task terminated unexpectedly: {err}"))?
}

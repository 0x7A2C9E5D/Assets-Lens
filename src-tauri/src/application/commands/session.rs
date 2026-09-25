//! The commands that pick the game data directory and bring a session back: auto-detect and manual
//! selection, the directory records left for the next launch, and the state (and cached index) a page
//! reads back when it opens.

use std::path::{Path, PathBuf};
use std::sync::{Arc, MutexGuard};

use maclarian::merged::GameDataResolver;
use tauri::{AppHandle, Manager};

use super::build::sorted_visual_ids;
use super::{current_stats, lock, SharedState};
use crate::application::cache::{self, Loaded, PersistedDatabase};
use crate::application::settings;
use crate::application::state::AppState;
use crate::domain::app::{AppSnapshot, CacheStatus};

/// Auto-detect the BG3 Data directory (default Steam install location).
///
/// Off the main thread like every other command that touches the disk: the probe walks the default
/// install locations and, once a directory is found, adopts and records it so the next launch comes
/// back to it without a probe, then reads back the index a previous build left for it.
#[tauri::command]
pub async fn detect_game_path(app: AppHandle) -> Result<Option<String>, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        let Some((path, resolver)) = detected_directory() else {
            return Ok(None);
        };
        adopt_directory(&state, path.clone(), resolver)?;
        finish_selection(&app, &state, &path);
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|err| format!("Auto-detect task terminated unexpectedly: {err}"))?
}

// The directory the default install locations point at, with the resolver opened on it. `None` when
// maclarian is unavailable at all or finds nothing there: an absent game is not an error, only a
// session that starts unconfigured
fn detected_directory() -> Option<(PathBuf, GameDataResolver)> {
    if !GameDataResolver::is_available() {
        return None;
    }
    match GameDataResolver::auto_detect() {
        Ok(resolver) => Some((resolver.game_data_path().to_path_buf(), resolver)),
        Err(err) => {
            eprintln!("[maclarian] auto-detect failed: {err}");
            None
        }
    }
}

// What follows a directory having been adopted: record it for the next launch, and bring back whatever
// index a previous build left for it
fn finish_selection(app: &AppHandle, state: &SharedState, dir: &Path) {
    record_directory(app, dir);
    restore_cached_database(app, state);
}

// Take `dir` and the resolver opened on it as this session's game data directory, dropping whatever
// index the previous one left.
//
// The lock is held around the switch alone: a build running concurrently has to find the new directory
// rather than the old one, and a directory change is the one moment the index is dead.
pub(super) fn adopt_directory(
    state: &SharedState,
    dir: PathBuf,
    resolver: GameDataResolver,
) -> Result<(), String> {
    let mut st = lock(state)?;
    st.game_path = Some(dir);
    st.resolver = Some(Arc::new(resolver));
    st.reset_index();
    Ok(())
}

// Adopt `dir` after checking it is a game data directory, opening its resolver. Both a directory the
// user picked and one a previous launch recorded come through here, so a recorded path that has since
// moved stops at the same check a wrong pick does
pub(super) fn open_directory(state: &SharedState, dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }
    if !dir.join("Shared.pak").exists() {
        return Err(format!(
            "Shared.pak not found in this directory. Please select the BG3 Data directory: {}",
            dir.display()
        ));
    }

    let resolver = GameDataResolver::new(dir)
        .map_err(|err| format!("Failed to initialize resource parser: {err}"))?;
    adopt_directory(state, dir.to_path_buf(), resolver)
}

// Record `dir` for the next launch. A failed write costs the record and nothing else — the directory
// is set for this session either way — so it is logged rather than handed to the caller
pub(super) fn record_directory(app: &AppHandle, dir: &Path) {
    if let Err(err) = settings::save(app, dir) {
        eprintln!("[settings] game directory not recorded: {err}");
    }
}

/// Manually set the BG3 Data directory (must contain Shared.pak)
#[tauri::command]
pub async fn set_game_path(app: AppHandle, path: String) -> Result<String, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let dir = PathBuf::from(&path);
        open_directory(&state, &dir)?;
        finish_selection(&app, &state, &dir);
        Ok(path)
    })
    .await
    .map_err(|err| format!("Directory change terminated unexpectedly: {err}"))?
}

/// Put the session back where the previous one left it, and report what it knows.
///
/// Called once when a page opens. The game data directory comes from the record a previous launch left
/// behind (`settings`) rather than from the frontend, and with it comes the index persisted for that
/// directory, so the statistics and the cache status of the `AppSnapshot` render without a second call.
/// A directory already adopted in this session is left as it is, and a recorded one that has since
/// moved or stopped holding game data is reported and dropped — the session then starts unset, as on a
/// first launch, instead of failing.
#[tauri::command]
pub async fn restore_state(app: AppHandle) -> Result<AppSnapshot, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<AppSnapshot, String> {
        // The lock is taken and released per question rather than held: every helper called here
        // takes it itself
        restore_recorded_directory(&app, &state)?;
        let st = lock(&state)?;
        Ok(snapshot(&st))
    })
    .await
    .map_err(|err| format!("State restore task terminated unexpectedly: {err}"))?
}

// Adopt the directory a previous launch recorded, when this session has not adopted one yet. Only a
// directory that was actually adopted has an index to read back; a recorded one that has since moved
// or stopped holding game data is reported and dropped
fn restore_recorded_directory(app: &AppHandle, state: &SharedState) -> Result<(), String> {
    if lock(state)?.game_path.is_some() {
        return Ok(());
    }
    let Some(dir) = settings::load(app) else {
        return Ok(());
    };

    match open_directory(state, &dir) {
        Ok(()) => restore_cached_database(app, state),
        Err(err) => eprintln!("[settings] {} not restored: {err}", dir.display()),
    }
    Ok(())
}

// What the page gets to render from: the directory in use (if any), the statistics of whatever index is
// there, and where that index came from
fn snapshot(st: &AppState) -> AppSnapshot {
    AppSnapshot {
        game_path: st.game_path.as_ref().map(|path| path.display().to_string()),
        stats: current_stats(st),
        cache: st.cache_status.clone(),
    }
}

// Fill the index for the current directory out of the file a previous build left for it.
//
// Called right after a directory has been selected, where `reset_index` has just emptied the state: a
// matching file turns a launch — or a switch back to a directory that was scanned before — into a read
// instead of a scan. The file is read with the state lock released, and the lock is taken again only to
// publish, because twenty-odd MB take long enough that holding it would stall every other command.
//
// Nothing here fails the caller: a file that cannot be read, or no longer matches the sources it was
// built from, only means there is nothing to restore, and the reason is reported through the state for
// the page to show (see `CacheStatus`).
pub(super) fn restore_cached_database(app: &AppHandle, state: &SharedState) {
    let Some(game_path) = current_game_path(state) else {
        return;
    };
    let (status, persisted) = read_cache(app, &game_path);

    let Some(mut st) = locked_state(state) else {
        return;
    };
    // A directory switch can land while the file is read: what came out of it then belongs to
    // resources that are no longer current
    if st.game_path.as_deref() != Some(game_path.as_path()) {
        return;
    }

    report_stale(&game_path, &status);
    publish_index(&mut st, persisted);
    st.cache_status = status;
}

// The directory this session is on, or `None` when none has been chosen yet — and when the state lock
// cannot be taken, which is logged as any other failed lock there
fn current_game_path(state: &SharedState) -> Option<PathBuf> {
    locked_state(state)?.game_path.clone()
}

// The state lock, or `None` when it cannot be taken (a poisoned mutex, which is logged here because
// every caller answers the same way)
fn locked_state(state: &SharedState) -> Option<MutexGuard<'_, AppState>> {
    match lock(state) {
        Ok(st) => Some(st),
        Err(err) => {
            eprintln!("[cache] {err}");
            None
        }
    }
}

// What the file for `game_path` says, as the status the page reports and the index to publish (when one
// came back)
fn read_cache(app: &AppHandle, game_path: &Path) -> (CacheStatus, Option<PersistedDatabase>) {
    match cache::load(app, game_path) {
        Loaded::Cache(persisted) => (CacheStatus::loaded(persisted.built_at), Some(*persisted)),
        Loaded::Missing => (CacheStatus::idle(), None),
        Loaded::Stale { code, detail } => (CacheStatus::stale(code, detail), None),
    }
}

// Report a file that was refused, with the reason it was refused for
fn report_stale(game_path: &Path, status: &CacheStatus) {
    let Some(code) = status.code.as_deref() else {
        return;
    };
    eprintln!(
        "[cache] {} not restored ({code}): {}",
        game_path.display(),
        status.detail.as_deref().unwrap_or("no detail")
    );
}

// Put a loaded index in place of whatever `reset_index` emptied. The three orders are derived rather
// than persisted: they are a sort of the ids that are in the database anyway, and recomputing them here
// keeps them consistent with the source map
fn publish_index(st: &mut AppState, persisted: Option<PersistedDatabase>) {
    let Some(persisted) = persisted else {
        return;
    };
    let (visual_ids, visual_ids_by_id, visual_ids_by_source) =
        sorted_visual_ids(&persisted.database, &persisted.mod_sources);
    st.visual_ids = visual_ids;
    st.visual_ids_by_id = visual_ids_by_id;
    st.visual_ids_by_source = visual_ids_by_source;
    st.materials = persisted.materials;
    st.mod_sources = persisted.mod_sources;
    st.merged_db = Some(persisted.database);
}

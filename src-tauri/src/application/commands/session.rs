//! The commands that pick the game data directory and bring a session back: auto-detect and manual
//! selection, the directory records left for the next launch, and the state (and cached index) a page
//! reads back when it opens.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use maclarian::merged::GameDataResolver;
use tauri::{AppHandle, Manager};

use super::build::sorted_visual_ids;
use super::{current_stats, lock, SharedState};
use crate::application::cache::{self, Loaded};
use crate::application::settings;
use crate::domain::app::{AppSnapshot, CacheStatus};

/// Auto-detect the BG3 Data directory (default Steam install location).
///
/// Off the main thread like every other command that touches the disk: the probe walks the default
/// install locations and, once a directory is found, adopts it — and records it, so the next launch
/// comes back to it without a probe (see `restore_state`) — before the index a previous build left
/// for it is read back (`restore_cached_database`).
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

        adopt_directory(&state, path.clone(), resolver)?;
        record_directory(&app, &path);

        // The directory has just been (re)selected, which is where the index for it can come back
        restore_cached_database(&app, &state);

        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|err| format!("Auto-detect task terminated unexpectedly: {err}"))?
}

/// Take `dir` and the resolver opened on it as this session's game data directory, dropping whatever
/// index the previous one left.
///
/// The lock is held around the switch alone: a build running concurrently has to find the new
/// directory rather than the old one, and a directory change is the one moment the index is dead.
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

/// Adopt `dir` after checking it is a game data directory, opening its resolver.
///
/// Both a directory the user picked and one a previous launch recorded come through here, so a
/// recorded path that has since moved stops at the same check a wrong pick does.
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

/// Record `dir` for the next launch. A failed write costs the record and nothing else — the directory
/// is set for this session either way — so it is logged rather than handed to the caller.
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
        record_directory(&app, &dir);

        // The directory has just been (re)selected, which is where the index for it can come back
        restore_cached_database(&app, &state);

        Ok(path)
    })
    .await
    .map_err(|err| format!("Directory change terminated unexpectedly: {err}"))?
}

/// Put the session back where the previous one left it, and report what it knows.
///
/// Called once when a page opens. The game data directory comes from the record a previous launch left
/// behind (`settings`) rather than from the frontend, and with it comes the index persisted for that
/// directory: the statistics and the cache status of the `AppSnapshot` are read after it, so the page
/// needs no further call to render.
///
/// A directory already adopted in this session (a build that ran before the page opened) is left as it
/// is, and a recorded directory that has since moved or stopped holding game data is reported and
/// dropped — the session then starts unset, exactly as on a first launch, instead of failing.
#[tauri::command]
pub async fn restore_state(app: AppHandle) -> Result<AppSnapshot, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<AppSnapshot, String> {
        // The lock is taken and released per question rather than held: `open_directory` and
        // `restore_cached_database` each take it themselves
        let adopted = lock(&state)?.game_path.is_some();

        if !adopted {
            if let Some(dir) = settings::load(&app) {
                match open_directory(&state, &dir) {
                    // Only a directory that was actually adopted has an index to read back
                    Ok(()) => restore_cached_database(&app, &state),
                    Err(err) => eprintln!("[settings] {} not restored: {err}", dir.display()),
                }
            }
        }

        let st = lock(&state)?;
        Ok(AppSnapshot {
            game_path: st.game_path.as_ref().map(|path| path.display().to_string()),
            stats: current_stats(&st),
            cache: st.cache_status.clone(),
        })
    })
    .await
    .map_err(|err| format!("State restore task terminated unexpectedly: {err}"))?
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
pub(super) fn restore_cached_database(app: &AppHandle, state: &SharedState) {
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

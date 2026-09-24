//! The little the app remembers between launches, kept apart from the index cache.
//!
//! The game data directory used to live in the frontend's Web storage, which made the frontend state
//! the only record of something the backend needs before it can answer anything. It is written here
//! instead — one small file in the app's data directory, next to the index cache — and read back by
//! `commands::restore_state` on startup. Which directories are still valid is not decided here: a
//! recorded path goes through the same check any chosen one does (see `commands::open_directory`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::application::cache;

/// The settings file. One field so far; serde keeps it readable and extensible.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    /// The BG3 Data directory, as it was last chosen
    game_path: Option<String>,
}

/// `settings.json` in the app's data directory — the same directory the index cache lives in
/// (`%APPDATA%\com.bg3.assets-lens` on Windows)
fn settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("App data directory unavailable: {err}"))?;
    Ok(dir.join("settings.json"))
}

/// The recorded game data directory, or `None` when nothing usable was recorded.
///
/// A file that cannot be read, or does not parse, counts as no record at all rather than an error:
/// the directory can always be chosen again, and failing a launch over a small settings file would be
/// a poor trade. Whether the path still is a game data directory is the caller's to decide.
pub fn load(app: &AppHandle) -> Option<PathBuf> {
    let file = settings_file(app).ok()?;
    let bytes = std::fs::read(&file).ok()?;
    let settings: Settings = serde_json::from_slice(&bytes).ok()?;
    Some(PathBuf::from(settings.game_path?))
}

/// Record `path` as the directory to come back to, replacing whatever was recorded before.
///
/// Written beside the file and renamed over it, through the same writer the index cache uses: a write
/// cut short then leaves the previous record intact instead of a truncated file.
pub fn save(app: &AppHandle, path: &Path) -> Result<(), String> {
    let file = settings_file(app)?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }

    let settings = Settings {
        game_path: Some(path.display().to_string()),
    };
    let temp = file.with_extension("json.tmp");
    let saved = cache::write_json(&temp, &settings).and_then(|()| {
        std::fs::rename(&temp, &file).map_err(|err| format!("{}: {err}", file.display()))
    });
    if saved.is_err() {
        // A half-written file must not be left lying around for the next launch to trip over
        let _ = std::fs::remove_file(&temp);
    }
    saved
}

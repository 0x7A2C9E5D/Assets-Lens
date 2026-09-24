//! What the app reports about itself: the progress of a build, the statistics and cache state of the
//! index it holds, and the envelope a page is answered with.

use serde::Serialize;

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub percent: f32,
}

/// Database statistics
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    pub visual_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

/// What the session knows about the index persisted on disk, so the Database page can say where its
/// statistics came from.
///
/// `state` is one of:
/// - `idle` — nothing to report: no directory selected yet, or the index was built in this session
/// - `loaded` — the index was read back from disk; `builtAt` is the unix time it was built at
/// - `stale` — a file is there but was refused; `code` says why
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    pub state: String,
    /// Unix seconds the persisted index was built at (only for `loaded`)
    pub built_at: Option<u64>,
    /// Stable reason a file was refused, for the frontend to map to copy: one of `version`,
    /// `game_paks`, `mod_paks`, `unreadable`
    pub code: Option<String>,
    /// Raw detail behind a `code` that needs one: the versions that wrote the file, or the parse error
    pub detail: Option<String>,
}

impl CacheStatus {
    /// Nothing to report
    pub fn idle() -> Self {
        Self {
            state: "idle".to_string(),
            built_at: None,
            code: None,
            detail: None,
        }
    }

    /// The index came back from disk
    pub fn loaded(built_at: u64) -> Self {
        Self {
            state: "loaded".to_string(),
            built_at: Some(built_at),
            code: None,
            detail: None,
        }
    }

    /// A persisted index was found but could not be used
    pub fn stale(code: &str, detail: Option<String>) -> Self {
        Self {
            state: "stale".to_string(),
            built_at: None,
            code: Some(code.to_string()),
            detail,
        }
    }
}

/// Everything a page needs to render without asking anything else: the game data directory this
/// session works on, the statistics of the index held for it, and where that index came from.
///
/// Returned once at startup (`commands::restore_state`) — the frontend used to assemble the same three
/// facts itself, from Web storage plus a comparison against the backend's directory.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// The game data directory in use, or `None` while none is set
    pub game_path: Option<String>,
    /// Statistics of the index held, or `None` when nothing is built yet
    pub stats: Option<DatabaseStats>,
    pub cache: CacheStatus,
}

/// App metadata for the About page
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Application version (compile-time, kept in sync with tauri.conf.json)
    pub version: String,
}

/// Generic pagination result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
}

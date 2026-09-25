//! What the command families have in common: the state handle they all take out of Tauri, the lock
//! helper that reports a poisoned mutex, and the few reads more than one family needs. Each family
//! lives in a submodule of its own, re-exported here so the frontend keeps the paths it registered.

use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::merged::VisualAsset;

use crate::application::state::AppState;
use crate::domain::app::DatabaseStats;

pub(crate) mod browse;
pub(crate) mod build;
pub(crate) mod detail;
pub(crate) mod export;
pub(crate) mod session;

pub(crate) use browse::{app_info, cache_status, db_stats, list_visuals};
pub(crate) use build::build_database;
pub(crate) use detail::{get_visual, get_visual_preview};
pub(crate) use export::export_visual_asset;
pub(crate) use session::{detect_game_path, restore_state, set_game_path};

pub type SharedState = Arc<Mutex<AppState>>;

// Lock the shared state; the only failure mode is a poisoned mutex, reported as a plain string
pub(crate) fn lock(state: &SharedState) -> Result<MutexGuard<'_, AppState>, String> {
    state
        .lock()
        .map_err(|e| format!("State lock unavailable: {e}"))
}

pub(crate) const NOT_CONFIGURED: &str =
    "BG3 Data directory is not set. Please use auto-detect or select a directory first.";
pub(crate) const NOT_BUILT: &str =
    "Resource database has not been built. Please go to the Database page and build it first.";

// The statistics of the index currently held, or `None` when nothing has been built yet. Read off the
// merged maps rather than `db.stats()`, because maclarian keeps the material map to itself and the
// material count can only come from the cache filled at build time
pub(crate) fn current_stats(st: &AppState) -> Option<DatabaseStats> {
    st.merged_db.as_ref().map(|db| DatabaseStats {
        // One entry per visual GUID, so the dashboard always matches the browse list
        visual_count: st.visual_ids.len(),
        material_count: st.materials.len(),
        texture_count: db.textures.len(),
        virtual_texture_count: db.virtual_textures.len(),
    })
}

// GTex hashes worth looking up among a visual's virtual textures. Blank hashes are dropped — they
// cannot match a page file, and looking them up would only re-list the archive — and each hash is
// lowercased because maclarian matches it against the page file name case-sensitively while the
// database does not promise a case
pub(crate) fn vt_hashes(asset: &VisualAsset) -> Vec<String> {
    asset
        .virtual_textures
        .iter()
        .map(|vt| vt.gtex_hash.trim().to_lowercase())
        .filter(|hash| !hash.is_empty())
        .collect()
}

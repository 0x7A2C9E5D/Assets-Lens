use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, MergedDatabase};

use crate::export::{build_gtp_index, lock_pool, Package};

/// Read preference for the shared PAK pool. Callers name the archive they expect (meshes from
/// `Models.pak`, a texture from its own archive), so this order only decides the fallback scan.
const PAK_PREFERENCE: &[&str] = &["Models.pak", "Textures.pak"];

/// Global application state: BG3 data directory, resource resolver, the built database,
/// and a stable name cache for consistent pagination order.
///
/// The game directory is not persisted here: the frontend remembers it (Web storage) and hands it
/// back through `set_game_path` on startup, so this state only lives for the current session.
pub struct AppState {
    /// Shared rather than owned: building the database runs for minutes, and the build has to keep
    /// working on the resolver after the state lock has been released (see `build_database`).
    /// It is not cloned — `GameDataResolver` is not `Clone`.
    pub resolver: Option<Arc<GameDataResolver>>,
    pub game_path: Option<std::path::PathBuf>,
    pub merged_db: Option<MergedDatabase>,
    /// Sorted visual names kept after building, so pagination order stays stable
    /// (HashMap iteration order is not deterministic)
    pub visual_names: Vec<String>,
    /// Index of `.gtp` paths across all PAK archives, looked up by GTex hash;
    /// built lazily on the first export that needs virtual textures
    pub texture_index: Option<Vec<String>>,
    /// PAK read pool shared by every command: opening an archive parses its whole file table, so the
    /// pool is created once per game directory instead of once per preview / export. `Mutex` because
    /// reading an archive needs `&mut` on its reader.
    pub packages: Option<Arc<Mutex<Package>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_names: Vec::new(),
            texture_index: None,
            packages: None,
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_names.clear();
        self.texture_index = None;
        // The archives still open belong to the previous directory
        self.packages = None;
    }

    /// The shared PAK pool, created on the first command that needs an archive. Callers clone the
    /// `Arc` back out and lock it only around a single read, so the state lock and the pool never
    /// have to be held at the same time.
    pub fn pool(&mut self) -> Result<Arc<Mutex<Package>>, String> {
        if let Some(pool) = &self.packages {
            return Ok(pool.clone());
        }

        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let pool = Arc::new(Mutex::new(Package::new(game_path, PAK_PREFERENCE)?));
        self.packages = Some(pool.clone());
        Ok(pool)
    }

    /// Return the virtual texture index. The first call scans the file tables of every PAK under
    /// the data directory and caches the result; the archives are huge and listing them repeatedly
    /// is expensive, so the index is built only once per session.
    pub fn gtp_index(&mut self) -> Vec<String> {
        if let Some(index) = &self.texture_index {
            return index.clone();
        }

        // A missing pool and a poisoned lock degrade the same way: the export simply runs without
        // virtual textures instead of failing, so both paths log and fall back to an empty index.
        let skipped = |err: String| {
            eprintln!("[maclarian] build GTP index failed: {err}");
            Vec::new()
        };
        let built = match self.pool() {
            Ok(pool) => lock_pool(&pool)
                .map(|mut pool| build_gtp_index(&mut pool))
                .unwrap_or_else(skipped),
            Err(err) => skipped(err),
        };
        self.texture_index = Some(built.clone());
        built
    }
}

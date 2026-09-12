use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, MergedDatabase};
use maclarian::pak::PakReaderCache;

use crate::export::{build_gtp_index, build_pak_index, main_paks, PakIndex};

/// How many archives maclarian's table cache keeps parsed: enough for every main archive of a full
/// install (~26), so a sweep leaves all the tables it walked resident instead of evicting them.
const CACHED_PAKS: usize = 32;

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
    pub game_path: Option<PathBuf>,
    pub merged_db: Option<MergedDatabase>,
    /// Sorted visual GUIDs kept after building, so pagination order stays stable
    /// (HashMap iteration order is not deterministic). Ids rather than names: a name can belong to
    /// several visuals, so keying the list by name would silently drop the duplicates.
    pub visual_ids: Vec<String>,
    /// The same GUIDs ordered by GUID: cached next to the name order so sorting the list by ID
    /// picks a sequence instead of re-sorting every id on each page request.
    pub visual_ids_by_id: Vec<String>,
    /// Index of the `.gtp` paths inside `VirtualTextures.pak`, looked up by GTex hash;
    /// built lazily on the first export that needs virtual textures
    pub texture_index: Option<Vec<String>>,
    /// Main archives in file-name order (data partitions excluded), so a read can walk them
    pub paks: Vec<PathBuf>,
    /// maclarian's PAK table cache shared by every command: opening an archive parses its whole file
    /// table, so the cache is created once per game directory instead of once per preview / export.
    /// `Mutex` because reading an archive needs `&mut`.
    pub cache: Option<Arc<Mutex<PakReaderCache>>>,
    /// Which archive holds each mesh and texture. Building it costs one pass over the file tables
    /// (~0.7s and ~40 MB for a full install), far too much for a single selected row but nothing
    /// once per session.
    pub pak_index: Option<PakIndex>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_ids: Vec::new(),
            visual_ids_by_id: Vec::new(),
            texture_index: None,
            paks: Vec::new(),
            cache: None,
            pak_index: None,
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_ids.clear();
        self.visual_ids_by_id.clear();
        self.texture_index = None;
        // The cached tables and the archive list still describe the previous directory
        self.paks.clear();
        self.cache = None;
        self.pak_index = None;
    }

    /// `VirtualTextures.pak` — the one archive that holds virtual textures
    pub fn vt_pak(&self) -> Option<PathBuf> {
        let path = self.game_path.as_ref()?.join("VirtualTextures.pak");
        path.is_file().then_some(path)
    }

    /// The shared archive list plus maclarian's table cache, created on the first command that needs
    /// an archive. Callers clone both back out and lock the cache only around a single read, so the
    /// state lock and the cache are never held at the same time.
    pub fn archives(&mut self) -> Result<(Arc<Mutex<PakReaderCache>>, Vec<PathBuf>), String> {
        if let Some(cache) = &self.cache {
            return Ok((cache.clone(), self.paks.clone()));
        }

        let game_path = self
            .game_path
            .clone()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let paks = main_paks(&game_path)?;
        let cache = Arc::new(Mutex::new(PakReaderCache::new(CACHED_PAKS)));
        self.paks = paks.clone();
        self.cache = Some(cache.clone());
        Ok((cache, paks))
    }

    /// Build the archive lookup index unless it is already there (see `pak_of`). Also called once
    /// when a database build finishes, so the first detail click does not have to pay for it.
    pub fn ensure_pak_index(&mut self) {
        if self.pak_index.is_some() {
            return;
        }

        // Without the archive list there is nothing to index; leave it unbuilt so a later call can
        // retry once a game directory is set
        let Ok((_, paks)) = self.archives() else {
            return;
        };
        self.pak_index = Some(build_pak_index(&paks));
    }

    /// Archive that holds `target` (file name only, empty when no archive lists it). The index is
    /// built on first use, so this is a map lookup on every later call: a row can be picked with the
    /// arrow keys without re-reading a single file table. A record spelled with `\` separators is
    /// retried once, since the tables always use `/`.
    pub fn pak_of(&mut self, target: &str) -> String {
        self.ensure_pak_index();

        let slashed = target.replace('\\', "/");
        let Some(index) = &self.pak_index else {
            return String::new();
        };
        index
            .name_of(target)
            .or_else(|| index.name_of(&slashed))
            .unwrap_or_default()
            .to_string()
    }

    /// Return the virtual texture index. The first call reads the file table of
    /// `VirtualTextures.pak` (the only archive that holds virtual textures) and caches the result;
    /// the archive is huge, so the index is built once per session rather than per export.
    pub fn gtp_index(&mut self) -> Vec<String> {
        if let Some(index) = &self.texture_index {
            return index.clone();
        }

        // A missing archive and an unreadable one degrade the same way: the export runs without
        // virtual textures instead of failing, so both paths log and fall back to an empty index.
        let built = match self.vt_pak() {
            Some(vt_pak) => build_gtp_index(&vt_pak),
            None => {
                eprintln!("[maclarian] VirtualTextures.pak not found; virtual textures are skipped");
                Vec::new()
            }
        };
        self.texture_index = Some(built.clone());
        built
    }
}

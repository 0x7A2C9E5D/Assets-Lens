use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, GtpMatch, MergedDatabase, MergedResolver};
use maclarian::pak::PakReaderCache;

use crate::archives::{build_pak_index, main_paks};
use crate::export::find_vt_matches;

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
    /// Main archives in file-name order (data partitions excluded), so a read can walk them
    pub paks: Vec<PathBuf>,
    /// maclarian's PAK table cache shared by every command: opening an archive parses its whole file
    /// table, so the cache is created once per game directory instead of once per preview / export.
    /// `Mutex` because reading an archive needs `&mut`.
    pub cache: Option<Arc<Mutex<PakReaderCache>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_ids: Vec::new(),
            visual_ids_by_id: Vec::new(),
            paks: Vec::new(),
            cache: None,
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_ids.clear();
        self.visual_ids_by_id.clear();
        // The cached tables and the archive list still describe the previous directory
        self.paks.clear();
        self.cache = None;
    }

    /// The archive maclarian's page-file lookup is allowed to search: `VirtualTextures.pak` inside the
    /// data directory the user configured.
    ///
    /// Deliberately not maclarian's `virtual_textures_pak_path()`: that one rebuilds the path from
    /// `bg3_data_path()`'s auto-detection, which comes back empty whenever the data directory was
    /// pointed at by hand. The archive only has to exist for the search — every read afterward takes
    /// its path out of the produced `GtpMatch` (`GtpMatch::pak_path`).
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

    /// Name the archive every mesh and texture of the built database came from, by filling
    /// maclarian's own `source_pak` fields (`VisualAsset::source_pak`, `TextureRef::source_pak`).
    ///
    /// Those fields are declared but never written by maclarian's parser, so the values come from
    /// listing the archive file tables (`build_pak_index`, ~0.7s for a full install) — a fraction of
    /// the minutes just spent parsing. Filling them once here is what makes the archive name free
    /// everywhere it is shown: the detail panel and `asset.json` both read the fields, and no click
    /// or export ever consults a file table for it. The index itself goes out of scope again, since
    /// it costs an order of magnitude more than the strings it hands out (`PakIndex`).
    ///
    /// Called with the database already stored, and only from `build_database`: without a database
    /// there is nothing to fill.
    pub fn fill_source_paks(&mut self) {
        // No archive list means no answers; leaving the fields empty is the honest outcome
        let Ok((_, paks)) = self.archives() else {
            return;
        };
        let index = build_pak_index(&paks);
        let Some(db) = self.merged_db.as_mut() else {
            return;
        };

        for visual in db.visuals_by_id.values_mut() {
            visual.source_pak = index.name_of(&visual.gr2_path).unwrap_or_default().to_string();
            // Each visual carries its own clone of the texture references it uses, so filling them
            // here covers every consumer; the texture bank itself (`db.textures`) is only consulted
            // while resolving, which is already done
            for texture in &mut visual.textures {
                texture.source_pak = index
                    .name_of(&texture.dds_path)
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }

    /// Resolve the `.gtp` page file behind each virtual texture hash, with maclarian's own lookup
    /// (`export::find_vt_matches`). This is the only way a virtual texture gets a location: the
    /// merged database stores the hash and nothing else.
    ///
    /// `MergedResolver` exists only for a database it owns, and this state holds the one database
    /// there is, so it is moved into the resolver and straight back out. That is a pointer move, not
    /// a copy, which is what keeps the official API affordable per request.
    ///
    /// Each match records the searched archive in `GtpMatch::pak_path`, and that field is the only
    /// archive the extraction and the manifest ever read: the archive is picked by the match here and
    /// nothing downstream rebuilds it from an archive's name.
    pub fn vt_matches(&mut self, hashes: &[&str]) -> Vec<GtpMatch> {
        // Without the one archive holding virtual textures there is nothing to look in, and taking
        // the database out first would only drop it on the floor
        let Some(vt_pak) = self.vt_pak() else {
            return Vec::new();
        };
        let Some(db) = self.merged_db.take() else {
            return Vec::new();
        };

        let resolver = MergedResolver::from_database(db);
        let matches = find_vt_matches(&resolver, hashes, &vt_pak);
        self.merged_db = Some(resolver.into_database());
        matches
    }
}

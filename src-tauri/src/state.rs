use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, GtpMatch, MergedDatabase, MergedResolver};

use crate::archives::Archives;
use crate::virtual_textures::PageFileSizes;

/// Read preference for the shared PAK pool. Callers name the archive they expect (meshes from
/// `Models.pak`, a texture from its own archive), so this order only decides the fallback scan.
const PAK_PREFERENCE: &[&str] = &["Models.pak", "Textures.pak"];

/// Archive holding the virtual texture page files. It is the one name the GTex lookup cannot get
/// from a match (the match is what it produces), so it is spelled out here.
const VIRTUAL_TEXTURES_PAK: &str = "VirtualTextures.pak";

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
    /// PAK read pool shared by every command: opening an archive parses its whole file table, so the
    /// pool is created once per game directory instead of once per preview / export. `Mutex` because
    /// reading an archive needs `&mut` on its reader.
    pub archives: Option<Arc<Mutex<Archives>>>,
    /// Page file sizes by GTS path, filled by `virtual_textures::page_file_size`. Reading a GTS
    /// costs a full extraction while one GTS serves every page file of its tile set, so browsing a
    /// model whose virtual textures share a tile set would otherwise read the same file repeatedly.
    /// An empty list is a cached "no sizes here" answer, not a missing entry.
    pub page_file_sizes: HashMap<String, PageFileSizes>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_ids: Vec::new(),
            visual_ids_by_id: Vec::new(),
            archives: None,
            page_file_sizes: HashMap::new(),
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_ids.clear();
        self.visual_ids_by_id.clear();
        // The archives still open belong to the previous directory
        self.archives = None;
        // Page file sizes were read from those archives
        self.page_file_sizes.clear();
    }

    /// The shared PAK pool, created on the first command that needs an archive. Callers clone the
    /// `Arc` back out and lock it only around a single read, so the state lock and the pool never
    /// have to be held at the same time.
    pub fn pool(&mut self) -> Result<Arc<Mutex<Archives>>, String> {
        if let Some(pool) = &self.archives {
            return Ok(pool.clone());
        }

        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let pool = Arc::new(Mutex::new(Archives::new(game_path, PAK_PREFERENCE)?));
        self.archives = Some(pool.clone());
        Ok(pool)
    }

    /// The archive that holds the virtual texture page files. maclarian's lookup needs one archive
    /// to list; which one that is cannot come from a page file (finding it is the lookup's job), so
    /// it is the single name this module spells out.
    pub fn vt_pak(&self) -> Result<PathBuf, String> {
        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let vt_pak = game_path.join(VIRTUAL_TEXTURES_PAK);
        if !vt_pak.is_file() {
            return Err(format!(
                "{VIRTUAL_TEXTURES_PAK} not found in {}",
                game_path.display()
            ));
        }
        Ok(vt_pak)
    }

    /// Resolve GTex hashes to their page files through maclarian's own lookup. Each `GtpMatch`
    /// carries the path inside the archive *and* the archive itself, so neither has to be guessed
    /// downstream — and unlike a hash-to-path index, a match can only exist for a hash that is
    /// actually present. A missing archive or a failed lookup degrades to "no virtual textures":
    /// they are optional, and must not abort a detail view or an export.
    ///
    /// `MergedResolver` owns the database it is built from, so the database moves out and back in
    /// (pointers move, nothing is copied); the state is left exactly as it was found.
    pub fn vt_matches(&mut self, hashes: &[String]) -> Vec<GtpMatch> {
        let Some(db) = self.merged_db.take() else {
            return Vec::new();
        };
        let pak = self.vt_pak();
        let resolver = MergedResolver::from_database(db);

        let matches = match pak {
            Ok(pak) => {
                let hashes: Vec<&str> = hashes.iter().map(String::as_str).collect();
                resolver
                    .find_gtp_by_hashes_in_pak(&hashes, &pak)
                    .unwrap_or_else(|err| {
                        eprintln!("[maclarian] virtual texture lookup failed: {err}");
                        Vec::new()
                    })
            }
            Err(err) => {
                eprintln!("[maclarian] {err}");
                Vec::new()
            }
        };

        self.merged_db = Some(resolver.into_database());
        matches
    }
}

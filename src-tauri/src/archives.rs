//! PAK archive access: the four archives the app reads from, opened on demand.
//!
//! Opening an archive parses its whole file table, so each one is opened once and kept resident.
//! Which archives those are is fixed by resource kind — a GR2 comes out of `Models.pak`, a DDS out
//! of `Textures.pak`, a material template out of `Materials.pak` and a virtual texture page file out
//! of `VirtualTextures.pak` — so the data directory is never walked looking for a file, and no read
//! falls back to another archive. Nothing here knows about assets or export formats: callers name
//! the archive they expect (`read_from` / `locate_many_in` / `list_in`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::pak::lspk::{FileTableEntry, LspkReader};

/// The archives the app reads from. The variant is the resource kind, not a directory listing:
/// every read names the kind it expects, which is what keeps a lookup from wandering into the
/// wrong archive (and from parsing a 12 GB file table to answer a question about a texture).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pak {
    /// GR2 meshes
    Models,
    /// DDS textures
    Textures,
    /// Material templates (`.lsf`) materials are derived from
    Materials,
    /// Virtual texture page files (`.gtp`) and the tile set metadata (`.gts`) beside them
    VirtualTextures,
}

impl Pak {
    /// File name inside the game's data directory
    pub fn file_name(self) -> &'static str {
        match self {
            Pak::Models => "Models.pak",
            Pak::Textures => "Textures.pak",
            Pak::Materials => "Materials.pak",
            Pak::VirtualTextures => "VirtualTextures.pak",
        }
    }
}

/// PAK read pool: each of the four archives is opened once and its file table and reader stay
/// resident, so indexes are never reparsed per file.
pub struct Archives {
    game_path: PathBuf,
    readers: HashMap<Pak, LspkReader<BufReader<File>>>,
    tables: HashMap<Pak, Vec<FileTableEntry>>,
}

/// Normalize a path: unify on `/` separators and lowercase for comparison
/// (separators inside PAK archives are inconsistent on Windows)
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

impl Archives {
    /// A pool over the fixed archive set in `game_path`; nothing is opened until it is first read
    pub fn new(game_path: &Path) -> Self {
        Self {
            game_path: game_path.to_path_buf(),
            readers: HashMap::new(),
            tables: HashMap::new(),
        }
    }

    /// Path of one archive inside the data directory
    fn path_of(&self, pak: Pak) -> PathBuf {
        self.game_path.join(pak.file_name())
    }

    /// Open an archive and cache its file table; no-op when already cached
    fn ensure(&mut self, pak: Pak) -> Result<(), String> {
        if self.tables.contains_key(&pak) {
            return Ok(());
        }

        let path = self.path_of(pak);
        let file =
            File::open(&path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
        let mut reader = LspkReader::with_path(BufReader::new(file), &path);
        let entries = reader
            .list_files()
            .map_err(|e| format!("Failed to read file table of {}: {e}", path.display()))?;

        self.readers.insert(pak, reader);
        self.tables.insert(pak, entries);
        Ok(())
    }

    /// Read file bytes out of one archive. The archive is named by kind, so a missing file is an
    /// error rather than the start of a search: `Models.pak` is the only archive a GR2 is read from
    pub fn read_from(&mut self, pak: Pak, target: &str) -> Result<Vec<u8>, String> {
        self.ensure(pak)?;

        let want = normalize_path(target);
        let entry = self
            .tables
            .get(&pak)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|e| normalize_path(&e.path.to_string_lossy()) == want)
            })
            .cloned()
            .ok_or_else(|| format!("{target} not found in {}", pak.file_name()))?;
        let reader = self
            .readers
            .get_mut(&pak)
            .ok_or_else(|| format!("Reader unavailable for {}", pak.file_name()))?;

        reader
            .decompress_file(&entry)
            .map_err(|e| format!("Failed to decompress {target}: {e}"))
    }

    /// List all file paths inside one archive (`/`-separated)
    pub fn list_in(&mut self, pak: Pak) -> Result<Vec<String>, String> {
        self.ensure(pak)?;
        Ok(self
            .tables
            .get(&pak)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| normalize_path(&e.path.to_string_lossy()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Resolve which of `targets` the archive `pak` holds, as `target -> archive file name`.
    ///
    /// The whole batch is answered by a single pass over that one archive's file table: a visual
    /// easily references a dozen textures, and rescanning a table with hundreds of thousands of
    /// entries per texture would be far too slow. Targets the archive does not hold come back
    /// absent, which the caller renders as "no archive" instead of a plausible-but-wrong name.
    /// Nothing is decompressed.
    pub fn locate_many_in(&mut self, pak: Pak, targets: &[String]) -> HashMap<String, String> {
        // Normalized path -> target exactly as the caller spelled it, so the result can be keyed by
        // the original string
        let mut pending: HashMap<String, String> = targets
            .iter()
            .filter(|target| !target.is_empty())
            .map(|target| (normalize_path(target), target.clone()))
            .collect();
        let mut located = HashMap::new();

        if pending.is_empty() || self.ensure(pak).is_err() {
            return located;
        }

        if let Some(entries) = self.tables.get(&pak) {
            for entry in entries {
                let path = normalize_path(&entry.path.to_string_lossy());
                if let Some(target) = pending.remove(&path) {
                    located.insert(target, pak.file_name().to_string());
                }
                if pending.is_empty() {
                    break;
                }
            }
        }

        located
    }
}

/// Lock the shared PAK pool for a single read
pub fn lock_pool(pool: &Arc<Mutex<Archives>>) -> Result<MutexGuard<'_, Archives>, String> {
    pool.lock()
        .map_err(|e| format!("PAK pool lock unavailable: {e}"))
}

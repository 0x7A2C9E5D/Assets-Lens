//! PAK archive access: the four archives the app reads from, opened on demand.
//!
//! Opening an archive parses its whole file table, so each one is opened once and kept resident.
//! Which archive holds what is fixed by resource kind, so nothing is ever searched across archives;
//! callers name the archive they expect (`read_from` / `list_in`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::pak::lspk::{FileTableEntry, LspkReader};

use crate::error::AppError;

/// The archives the app reads from. The variant is the resource kind, so every read names the
/// archive it expects instead of searching for one.
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
    fn ensure(&mut self, pak: Pak) -> Result<(), AppError> {
        if self.tables.contains_key(&pak) {
            return Ok(());
        }

        let path = self.path_of(pak);
        let file = File::open(&path).map_err(|e| AppError::ArchiveOpen {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        let mut reader = LspkReader::with_path(BufReader::new(file), &path);
        let entries = reader
            .list_files()
            .map_err(|e| AppError::ArchiveFileTable {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;

        self.readers.insert(pak, reader);
        self.tables.insert(pak, entries);
        Ok(())
    }

    /// Read file bytes out of one archive; a missing file is an error, not the start of a search.
    pub fn read_from(&mut self, pak: Pak, target: &str) -> Result<Vec<u8>, AppError> {
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
            .ok_or_else(|| AppError::ArchiveEntryMissing {
                target: target.to_string(),
                pak: pak.file_name(),
            })?;
        let reader = self
            .readers
            .get_mut(&pak)
            .ok_or(AppError::ArchiveReaderMissing {
                pak: pak.file_name(),
            })?;

        reader
            .decompress_file(&entry)
            .map_err(|e| AppError::ArchiveDecompress {
                target: target.to_string(),
                detail: e.to_string(),
            })
    }

    /// List all file paths inside one archive (`/`-separated)
    pub fn list_in(&mut self, pak: Pak) -> Result<Vec<String>, AppError> {
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
}

/// Lock the shared PAK pool for a single read
pub fn lock_pool(pool: &Arc<Mutex<Archives>>) -> Result<MutexGuard<'_, Archives>, AppError> {
    pool.lock().map_err(|e| AppError::ArchivePoolLock {
        detail: e.to_string(),
    })
}

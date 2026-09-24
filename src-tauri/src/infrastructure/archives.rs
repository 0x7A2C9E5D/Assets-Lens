//! PAK archive access: the four game archives plus any mod archive the app was pointed at, opened
//! on demand.
//!
//! Opening an archive parses its whole file table, so each one is opened once and kept resident.
//! The game's four are fixed by resource kind — a GR2 comes out of `Models.pak`, a DDS out of
//! `Textures.pak`, a material template out of `Materials.pak` and a virtual texture page file out of
//! `VirtualTextures.pak` — so the data directory is never walked looking for a file. Mod archives are
//! the one addition: they are read first, because a mod ships replacements for files the game also
//! has, and their resources are spread over whichever archive the mod author chose. Nothing here
//! knows about assets or export formats: callers name the archive they expect (`read_from` /
//! `list_in`), or name a mod by index (`read_mod_file` / `list_mod`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::pak::lspk::{FileTableEntry, LspkReader};

/// The game's archives. The variant is the resource kind, not a directory listing: every read names
/// the kind it expects, which is what keeps a lookup from wandering into the wrong archive (and from
/// parsing a 12 GB file table to answer a question about a texture).
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

/// One mod archive, opened on demand like the game's own
struct ModPak {
    reader: LspkReader<BufReader<File>>,
    table: Vec<FileTableEntry>,
}

/// PAK read pool: the game's four archives and any mod archives are opened once each, and their file
/// tables and readers stay resident, so indexes are never reparsed per file.
pub struct Archives {
    game_path: PathBuf,
    /// The mod archives to search, in ascending file name order — the order they are searched in
    /// reversed, so the last one wins. A mod that replaces something is meant to take precedence over
    /// both the game and the mods loaded before it.
    mod_paths: Vec<PathBuf>,
    readers: HashMap<Pak, LspkReader<BufReader<File>>>,
    tables: HashMap<Pak, Vec<FileTableEntry>>,
    /// One slot per entry of `mod_paths`, filled on first use
    mod_paks: Vec<Option<ModPak>>,
}

/// Normalize a path: unify on `/` separators and lowercase for comparison
/// (separators inside PAK archives are inconsistent on Windows)
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// The entry whose path matches `want`, compared through `normalize_path` so separators and casing
/// do not have to agree with the archive
fn find_entry(table: &[FileTableEntry], want: &str) -> Option<FileTableEntry> {
    table
        .iter()
        .find(|entry| normalize_path(&entry.path.to_string_lossy()) == want)
        .cloned()
}

impl Archives {
    /// A pool over the game's archives in `game_path` plus `mod_paths`; nothing is opened until it is
    /// first read
    pub fn new(game_path: &Path, mod_paths: Vec<PathBuf>) -> Self {
        let mod_paks = mod_paths.iter().map(|_| None).collect();
        Self {
            game_path: game_path.to_path_buf(),
            mod_paths,
            readers: HashMap::new(),
            tables: HashMap::new(),
            mod_paks,
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

    /// Open one mod archive and cache its file table; no-op when already cached
    fn ensure_mod(&mut self, index: usize) -> Result<(), String> {
        if self.mod_paks.get(index).is_some_and(Option::is_some) {
            return Ok(());
        }

        let path = self
            .mod_paths
            .get(index)
            .cloned()
            .ok_or_else(|| format!("Mod archive #{index} is not part of the pool"))?;
        let file =
            File::open(&path).map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
        let mut reader = LspkReader::with_path(BufReader::new(file), &path);
        let table = reader
            .list_files()
            .map_err(|e| format!("Failed to read file table of {}: {e}", path.display()))?;

        self.mod_paks[index] = Some(ModPak { reader, table });
        Ok(())
    }

    /// Read one file out of a mod archive, or nothing when that mod does not carry it
    fn read_mod(
        &mut self,
        index: usize,
        target: &str,
        want: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        self.ensure_mod(index)?;

        let Some(mod_pak) = self.mod_paks.get_mut(index).and_then(Option::as_mut) else {
            return Ok(None);
        };
        let Some(entry) = find_entry(&mod_pak.table, want) else {
            return Ok(None);
        };

        mod_pak
            .reader
            .decompress_file(&entry)
            .map(Some)
            .map_err(|e| format!("Failed to decompress {target}: {e}"))
    }

    /// Read file bytes out of one archive.
    ///
    /// Mod archives are searched first, highest priority first: a mod ships replacements for files the
    /// game also has, and a DDS a modded visual points at has to come out of the mod. Only then is the
    /// archive named by kind read, where a missing file is an error rather than the start of a search.
    ///
    /// A mod archive that cannot be opened is skipped rather than reported: it must not keep the
    /// game's own files from being read.
    pub fn read_from(&mut self, pak: Pak, target: &str) -> Result<Vec<u8>, String> {
        let want = normalize_path(target);

        for index in (0..self.mod_paths.len()).rev() {
            match self.read_mod(index, target, &want) {
                Ok(Some(bytes)) => return Ok(bytes),
                Ok(None) => {}
                Err(err) => eprintln!("[maclarian] mod archive skipped: {err}"),
            }
        }

        self.ensure(pak)?;

        let entry = self
            .tables
            .get(&pak)
            .and_then(|entries| find_entry(entries, &want))
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

    /// How many mod archives the pool was built with
    pub fn mod_count(&self) -> usize {
        self.mod_paths.len()
    }

    /// Name of a mod: its archive's file name without the extension, which is what the UI labels the
    /// resources it provides with
    pub fn mod_name(&self, index: usize) -> String {
        self.mod_paths
            .get(index)
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// List all file paths inside one mod archive (`/`-separated)
    pub fn list_mod(&mut self, index: usize) -> Result<Vec<String>, String> {
        self.ensure_mod(index)?;
        Ok(self
            .mod_paks
            .get(index)
            .and_then(Option::as_ref)
            .map(|mod_pak| {
                mod_pak
                    .table
                    .iter()
                    .map(|e| normalize_path(&e.path.to_string_lossy()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Read file bytes out of one mod archive. Unlike `read_from`, this names the mod directly, so
    /// the file is looked up in that one archive and nowhere else.
    pub fn read_mod_file(&mut self, index: usize, target: &str) -> Result<Vec<u8>, String> {
        let name = self.mod_name(index);
        self.read_mod(index, target, &normalize_path(target))?
            .ok_or_else(|| format!("{target} not found in {name}"))
    }
}

/// Lock the shared PAK pool for a single read
pub fn lock_pool(pool: &Arc<Mutex<Archives>>) -> Result<MutexGuard<'_, Archives>, String> {
    pool.lock()
        .map_err(|e| format!("PAK pool lock unavailable: {e}"))
}

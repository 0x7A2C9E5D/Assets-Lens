//! PAK archive access: a read pool over the game's LSPK archives.
//!
//! Opening an archive parses its whole file table, so each one is opened once and kept resident
//! (`Archives`). Nothing here knows about assets or export formats: callers either ask for file
//! bytes (`read_in` / `read`) or for the archive a path belongs to (`locate_many`).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use maclarian::pak::lspk::{FileTableEntry, LspkReader};

/// How many archives the pool keeps open at most. A cached archive pins a file handle plus its whole
/// file table, and a fallback scan touches every PAK in the game directory, so the cache is capped
/// and dropped wholesale once it is full.
const MAX_CACHED_PAKS: usize = 6;

/// PAK read pool: each PAK is opened once and its file table and reader stay resident, so indexes
/// are never reparsed per file.
pub struct Archives {
    /// Game archives, sorted by read priority (earlier entries are tried first)
    paks: Vec<PathBuf>,
    readers: HashMap<PathBuf, LspkReader<BufReader<File>>>,
    tables: HashMap<PathBuf, Vec<FileTableEntry>>,
}

/// True when `file_name` is a LSPK data-partition archive (`<Name>_<n>.pak`, e.g.
/// `VirtualTextures_12.pak`). BG3 splits large resources over numbered archives that only hold
/// raw data blocks — they carry no LSPK header of their own, cannot be opened standalone, and are
/// already reachable through their main (`<Name>.pak`) archive, so they are excluded up front.
fn is_data_partition(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    let stem = lower.strip_suffix(".pak").unwrap_or(&lower);
    match stem.rsplit_once('_') {
        Some((_, tail)) => !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// Normalize a path: unify on `/` separators and lowercase for comparison
/// (separators inside PAK archives are inconsistent on Windows)
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

impl Archives {
    /// List every main `.pak` under the data directory, hoisting the names in `prefer` to the front
    /// (e.g. `Models.pak` / `Textures.pak`). Numbered data partitions (`<Name>_<n>.pak`) are
    /// excluded — see `is_data_partition`.
    pub fn new(game_path: &Path, prefer: &[&str]) -> Result<Self, String> {
        let mut paks: Vec<PathBuf> = fs::read_dir(game_path)
            .map_err(|e| format!("Failed to list BG3 data directory: {e}"))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().and_then(|s| s.to_str()) == Some("pak")
                    && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_data_partition)
            })
            .collect();

        paks.sort_by_key(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            prefer
                .iter()
                .position(|want| name.eq_ignore_ascii_case(want))
                .unwrap_or(prefer.len())
        });

        Ok(Self {
            paks,
            readers: HashMap::new(),
            tables: HashMap::new(),
        })
    }

    /// Open a PAK and cache its file table; no-op when already cached
    fn ensure(&mut self, pak: &Path) -> Result<(), String> {
        if self.tables.contains_key(pak) {
            return Ok(());
        }

        if self.readers.len() >= MAX_CACHED_PAKS {
            // Over the cap: drop the cache rather than grow it, so one full scan cannot pin every
            // archive in the game directory. The hot ones are re-opened on the next read.
            self.readers.clear();
            self.tables.clear();
        }

        let file = File::open(pak).map_err(|e| format!("Failed to open {}: {e}", pak.display()))?;
        let mut reader = LspkReader::with_path(BufReader::new(file), pak);
        let entries = reader
            .list_files()
            .map_err(|e| format!("Failed to read file table of {}: {e}", pak.display()))?;

        self.readers.insert(pak.to_path_buf(), reader);
        self.tables.insert(pak.to_path_buf(), entries);
        Ok(())
    }

    /// List all file paths inside a PAK (`/`-separated)
    pub fn list(&mut self, pak: &Path) -> Result<Vec<String>, String> {
        self.ensure(pak)?;
        Ok(self
            .tables
            .get(pak)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| normalize_path(&e.path.to_string_lossy()))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Read file bytes from a specific PAK
    pub fn read_in(&mut self, pak: &Path, target: &str) -> Result<Vec<u8>, String> {
        self.ensure(pak)?;

        let want = normalize_path(target);
        let index = self
            .tables
            .get(pak)
            .and_then(|entries| {
                entries
                    .iter()
                    .position(|e| normalize_path(&e.path.to_string_lossy()) == want)
            })
            .ok_or_else(|| format!("{target} not found in {}", pak.display()))?;

        let entry = self
            .tables
            .get(pak)
            .and_then(|entries| entries.get(index).cloned())
            .ok_or_else(|| format!("{target} not found in {}", pak.display()))?;
        let reader = self
            .readers
            .get_mut(pak)
            .ok_or_else(|| format!("Reader unavailable for {}", pak.display()))?;

        reader
            .decompress_file(&entry)
            .map_err(|e| format!("Failed to decompress {target}: {e}"))
    }

    /// Read a file from any PAK under the data directory, honoring priority.
    /// When `prefer` is given, that PAK is tried first (e.g. GR2 in `Models.pak`,
    /// DDS in `Textures.pak`); otherwise every PAK is scanned in order.
    pub fn read(&mut self, target: &str, prefer: Option<&str>) -> Result<Vec<u8>, String> {
        if let Some(want) = prefer {
            let preferred = self.paks.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(want))
            });
            if let Some(pak) = preferred.cloned() {
                if let Ok(bytes) = self.read_in(&pak, target) {
                    return Ok(bytes);
                }
            }
        }

        for pak in self.paks.clone() {
            if let Ok(bytes) = self.read_in(&pak, target) {
                return Ok(bytes);
            }
        }

        Err(format!("{target} not found in BG3 archives"))
    }

    /// List file paths across *all* main PAKs in the data directory, deduplicated.
    /// Numbered data partitions were already excluded in `new` (see `is_data_partition`); the
    /// `skipping unreadable` branch below only fires for genuinely broken or foreign archives.
    pub fn list_all(&mut self) -> Result<Vec<String>, String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for pak in self.paks.clone() {
            if let Ok(entries) = self.list(&pak) {
                for entry in entries {
                    if seen.insert(entry.clone()) {
                        out.push(entry);
                    }
                }
            } else {
                eprintln!(
                    "[maclarian] skipping unreadable / non-LSPK archive: {}",
                    pak.display()
                );
            }
        }
        Ok(out)
    }

    /// Resolve which archive actually holds each of `targets`, as `target -> archive file name`.
    ///
    /// The whole batch is answered by a single pass over the cached file tables: a visual easily
    /// references a dozen textures, and rescanning a table with hundreds of thousands of entries per
    /// texture would be far too slow. Priority follows the pool's own order — the same rule `read`
    /// applies, so the first archive containing a path wins. Nothing is decompressed.
    pub fn locate_many(&mut self, targets: &[String]) -> HashMap<String, String> {
        // Normalized path -> target exactly as the caller spelled it, so the result can be keyed by
        // the original string
        let mut pending: HashMap<String, String> = targets
            .iter()
            .filter(|target| !target.is_empty())
            .map(|target| (normalize_path(target), target.clone()))
            .collect();
        let mut located = HashMap::new();

        for pak in self.paks.clone() {
            if pending.is_empty() {
                break;
            }
            if self.ensure(&pak).is_err() {
                continue;
            }
            let name = match pak.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if let Some(entries) = self.tables.get(&pak) {
                for entry in entries {
                    let path = normalize_path(&entry.path.to_string_lossy());
                    if let Some(target) = pending.remove(&path) {
                        located.insert(target, name.clone());
                    }
                    if pending.is_empty() {
                        break;
                    }
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

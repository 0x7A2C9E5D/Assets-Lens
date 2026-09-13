//! The data directory's archive inventory: which main archives exist, and which one holds a path.
//!
//! Neither neighbour's job. `export.rs` is a pipeline that runs on what it is handed (`source` +
//! `path`) and never walks the game directory; `state.rs` only remembers the list that
//! `AppState::archives()` hands out. Listing the directory and answering "which archive holds this
//! path" is a concern of its own, so it lives here.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use maclarian::pak::PakOperations;

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

/// List every main `.pak` under the data directory in file-name order. Numbered data partitions
/// (`<Name>_<n>.pak`) are excluded — see `is_data_partition`.
///
/// No archive is promoted: meshes, textures and virtual textures are all looked up the same way,
/// so a "preferred" archive would only decide which copy wins when a path exists in several
/// archives, and never which archives get searched.
pub fn main_paks(game_path: &Path) -> Result<Vec<PathBuf>, String> {
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

    // Deterministic order only: `read_dir` order is arbitrary, and which archive answers a lookup
    // has to stay stable between runs.
    paks.sort();
    Ok(paks)
}

/// Which archive holds a mesh or a texture: the only two file kinds whose archive is ever named (the
/// detail panel and `asset.json`), and therefore the only two that are indexed.
///
/// Built with maclarian's own API: its `extract_dds_textures` answers the same question the same
/// way, by listing every archive with `PakOperations::list` and testing containment. The crate has
/// no cheaper reverse lookup — `PakReaderCache` keeps its tables private, and its parser never fills
/// the `source_pak` fields — and listing an archive costs a full table read.
///
/// The tables' own spelling is the key, which is exactly how the database spells its paths. The
/// first archive to list a path wins, i.e. the order `export::read_file` sweeps in, so a label can
/// never contradict where the bytes actually come from.
///
/// Filling the database's `source_pak` fields is all this is for, so it is built after a database
/// build and dropped right after (`AppState::fill_source_paks`) — holding ~40 MB for the whole
/// session to answer lookups that the filled fields already answer would be waste. A full installation
/// holds 224560 meshes and textures out of 567681 entries; indexing the rest (sound banks, layouts,
/// virtual texture pages) would triple the peak for paths nobody asks about, and storing the archive
/// name per entry instead of an index into `names` would double it again.
pub struct PakIndex {
    names: Vec<String>,
    by_path: HashMap<Box<str>, u16>,
}

impl PakIndex {
    /// Archive file name holding `target`, spelled the way the file tables spell it: `/` separators,
    /// original casing. A record spelled with `\` separators is retried once.
    pub fn name_of(&self, target: &str) -> Option<&str> {
        self.lookup(target)
            .or_else(|| self.lookup(&target.replace('\\', "/")))
    }

    fn lookup(&self, target: &str) -> Option<&str> {
        let slot = *self.by_path.get(target)?;
        self.names.get(slot as usize).map(String::as_str)
    }
}

/// Whether `path` names a mesh or a texture, i.e. something whose archive is named (see `PakIndex`)
fn is_mesh_or_texture(path: &str) -> bool {
    path.get(path.len().saturating_sub(4)..).is_some_and(|ext| {
        ext.eq_ignore_ascii_case(".gr2") || ext.eq_ignore_ascii_case(".dds")
    })
}

/// List every main archive and record which one holds which mesh / texture path
pub fn build_pak_index(paks: &[PathBuf]) -> PakIndex {
    let names: Vec<String> = paks
        .iter()
        .map(|pak| {
            pak.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .collect();

    let mut by_path: HashMap<Box<str>, u16> = HashMap::new();
    // A full installation has a couple of dozen archives, so the index fits in a `u16` with room to spare
    for (slot, pak) in paks.iter().enumerate() {
        match PakOperations::list(pak) {
            Ok(entries) => {
                for path in entries {
                    if is_mesh_or_texture(&path) {
                        by_path.entry(path.into_boxed_str()).or_insert(slot as u16);
                    }
                }
            }
            Err(err) => eprintln!("[maclarian] listing {} failed: {err}", pak.display()),
        }
    }

    PakIndex { names, by_path }
}

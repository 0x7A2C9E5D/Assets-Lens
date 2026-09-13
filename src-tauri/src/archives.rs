//! The data directory's archive inventory: which archive holds a path.
//!
//! One question is asked of it — after a database build, which archive holds each mesh and each
//! texture (`AppState::fill_source_paks`) — and that is neither neighbour's job: `export.rs` runs on
//! the `source` + `path` it is handed and never walks the game directory, and `state.rs` only
//! remembers the list `AppState::archives()` hands out.
//!
//! The archive list itself is maclarian's own directory scan (`find_pak_files`, see
//! `AppState::archives`); this module only builds the reverse index over it.

use std::collections::HashMap;
use std::path::PathBuf;

use maclarian::pak::PakOperations;

/// Whether `path` names a mesh or a texture, i.e. something whose archive is named (see `PakIndex`)
fn is_mesh_or_texture(path: &str) -> bool {
    path.get(path.len().saturating_sub(4)..).is_some_and(|ext| {
        ext.eq_ignore_ascii_case(".gr2") || ext.eq_ignore_ascii_case(".dds")
    })
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
/// first archive to list a path wins, i.e. the order `export::read_file` sweeps in (both walk the
/// list `AppState::archives()` returned), so a label can never contradict where the bytes actually
/// come from.
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

/// List every archive and record which one holds which mesh / texture path.
///
/// `on_pak` is called after each archive with `(listed, total)`: this pass is longer than the parse
/// that precedes a build, so a caller that shows progress has to be able to report it.
pub fn build_pak_index(paks: &[PathBuf], on_pak: &dyn Fn(usize, usize)) -> PakIndex {
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
        // Nothing is filtered up front: an archive that cannot be opened (a data partition, a
        // localization archive) answers nothing here, the same free skip every read gets
        if let Ok(entries) = PakOperations::list(pak) {
            for path in entries {
                if is_mesh_or_texture(&path) {
                    by_path.entry(path.into_boxed_str()).or_insert(slot as u16);
                }
            }
        }
        on_pak(slot + 1, paks.len());
    }

    PakIndex { names, by_path }
}

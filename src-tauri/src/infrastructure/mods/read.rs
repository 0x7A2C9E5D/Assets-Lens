//! Reading a mod's banks: which of its archive entries are banks, and what each one parses into.

use std::collections::BTreeMap;

use maclarian::converter::to_lsx;
use maclarian::formats::lsf::parse_lsf_bytes;
use maclarian::formats::lsx::{parse_lsx, LsxDocument, LsxNode, LsxRegion};

use crate::infrastructure::archives::Archives;

use super::naming::mod_display_name;
use super::ModAssets;

/// Read one mod archive into the resources it contributes.
///
/// A file that fails to parse is skipped with a note: a mod is third-party data, and one unreadable
/// bank must not cost the whole build. Only the module's bank folders are walked (see `bank_dir`), one
/// directory at a time, so a `_merged.lsf` can stand in for the files beside it.
pub fn read_mod(archives: &mut Archives, index: usize) -> ModAssets {
    let stem = archives.mod_name(index);
    let Some(paths) = mod_paths(archives, index, &stem) else {
        return ModAssets::new(stem);
    };

    let name = mod_display_name(archives, index, &paths, stem);
    let mut assets = ModAssets::new(name);
    for path in bank_files(&paths) {
        read_banks(archives, index, path, &mut assets);
    }
    assets
}

// The archive entries of one mod, or `None` — reported — when its file table cannot be listed
fn mod_paths(archives: &mut Archives, index: usize, stem: &str) -> Option<Vec<String>> {
    match archives.list_mod(index) {
        Ok(paths) => Some(paths),
        Err(err) => {
            eprintln!("[maclarian] mod {stem} skipped: {err}");
            None
        }
    }
}

// The bank files of one mod, grouped one directory at a time and in directory order
fn bank_files(paths: &[String]) -> Vec<&str> {
    let mut banks: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in paths {
        if let Some(dir) = bank_dir(path) {
            banks.entry(dir).or_default().push(path);
        }
    }

    banks.into_values().flat_map(without_merged_siblings).collect()
}

// One bank directory's files, with the `.lsf` banks a `_merged.lsf` beside them stands for dropped:
// that file already holds every one of them. `.lsx` banks stay, the merged file accounts for the
// `.lsf` ones only.
fn without_merged_siblings(mut files: Vec<&str>) -> Vec<&str> {
    if files.iter().any(|path| path.ends_with("_merged.lsf")) {
        files.retain(|path| path.ends_with("_merged.lsf") || path.ends_with(".lsx"));
    }
    files
}

// Read every indexed bank region of one file into `assets`; a region that carries no indexed
// resource is skipped
fn read_banks(archives: &mut Archives, index: usize, path: &str, assets: &mut ModAssets) {
    let Some(lsx) = read_bank(archives, index, path) else {
        return;
    };
    for region in &lsx.regions {
        match region.id.as_str() {
            "VisualBank" => assets.read_visuals(region),
            "MaterialBank" => assets.read_materials(region),
            "TextureBank" => assets.read_textures(region),
            "VirtualTextureBank" => assets.read_virtual_textures(region),
            // Every other region (Templates, Tags, CharacterVisualBank, …) carries no indexed
            // resource
            _ => {}
        }
    }
}

// The bank directory an archive entry belongs to, or `None` when the entry is not a bank file.
//
// Two things have to hold: the path sits under `Public/` and carries a `Content` folder, and a
// `[PAK]_<name>` folder appears in it. Neither has to be the file's own folder — the engine nests banks
// under the asset tree they belong to, so folders may sit between `Content` and `[PAK]_`, and a bank
// file may sit below `[PAK]_` in turn. Both `.lsf` and `.lsx` banks are read: the editor emits either
// form, and both carry the same regions.
//
// Everything else a module ships supplies no indexed resource: `RootTemplates` and `Tags` hold template
// and tag tables, `GUI` and the `Mods/<name>/` metadata live outside `Content` altogether. Rejecting
// them by path keeps those files from being parsed only for every region in them to be dropped.
fn bank_dir(path: &str) -> Option<&str> {
    let (dir, file) = path.rsplit_once('/')?;
    if !matches!(file.rsplit('.').next(), Some("lsf" | "lsx")) {
        return None;
    }

    let folders = dir.strip_prefix("public/")?;
    let is_content = |folder: &str| folder == "content";
    let is_bank = |folder: &str| folder.starts_with("[pak]_");
    let has_content = folders.split('/').any(is_content);
    let has_bank = folders.split('/').any(is_bank);

    (has_content && has_bank).then_some(dir)
}

// Parse one bank file into an LSX document; a file that fails any step is skipped, leaving the mod's
// other banks unaffected.
pub(super) fn read_bank(archives: &mut Archives, index: usize, path: &str) -> Option<LsxDocument> {
    let bytes = mod_file_bytes(archives, index, path)?;
    let xml = bank_xml(path, &bytes)?;
    parse_lsx(&xml)
        .map_err(|err| eprintln!("[maclarian] {path} is not readable as LSX: {err}"))
        .ok()
}

// The bytes of one mod file, or `None` — reported — when that file cannot be read
fn mod_file_bytes(archives: &mut Archives, index: usize, path: &str) -> Option<Vec<u8>> {
    match archives.read_mod_file(index, path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            eprintln!("[maclarian] mod file skipped: {err}");
            None
        }
    }
}

// The XML of one bank file, or `None` — reported — when its own format cannot be read: a `.lsx` bank is
// XML already, a `.lsf` bank goes through maclarian's LSF reader and then its converter, which hands
// over the same document model. A file that fails any step is skipped, leaving the mod's other banks
// unaffected.
fn bank_xml(path: &str, bytes: &[u8]) -> Option<String> {
    if path.ends_with(".lsx") {
        return lsx_text(path, bytes);
    }
    parse_lsf_bytes(bytes)
        .and_then(|document| to_lsx(&document))
        .map_err(|err| eprintln!("[maclarian] {path} is not readable as LSF: {err}"))
        .ok()
}

// One `.lsx` bank's text, or `None` — reported — when the bytes are not valid UTF-8
fn lsx_text(path: &str, bytes: &[u8]) -> Option<String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|err| eprintln!("[maclarian] {path} is not readable as LSX: {err}"))
        .ok()
}

// The `Resource` nodes of a bank region: a per-resource bank file nests them under a node named after
// the bank (`VisualBank` → `Resource`), which is the shape maclarian reads out of a merged file, and a
// region listing them directly is accepted as well.
pub(super) fn resource_nodes<'a>(region: &'a LsxRegion, bank: &str) -> Vec<&'a LsxNode> {
    let mut resources = Vec::new();
    for node in &region.nodes {
        if node.id == bank {
            resources.extend(node.children.iter().filter(|child| child.id == "Resource"));
        } else if node.id == "Resource" {
            resources.push(node);
        }
    }
    resources
}

// Value of one attribute of a node; a node that does not carry it yields an empty string, which is what
// every caller tests against
pub(super) fn attr(node: &LsxNode, id: &str) -> String {
    node.attributes
        .iter()
        .find(|attr| attr.id == id)
        .map(|attr| attr.value.clone())
        .unwrap_or_default()
}

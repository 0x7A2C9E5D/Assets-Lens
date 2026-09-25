//! Staging the page files of a virtual texture: writing the page file a `GtpMatch` names plus the
//! GTS that covers it out of the archives into a shared temporary directory.

use std::fs;
use std::path::{Path, PathBuf};

use maclarian::merged::GtpMatch;

use crate::domain::naming::derive_gts_path;
use crate::infrastructure::archives::{Archives, Pak};

/// Files staged for one-page file, both already on disk
pub struct StagedSources {
    /// Page file (`.gtp`), named after its archive entry
    pub gtp: PathBuf,
    /// Candidate GTS files, ordered by likelihood. Normally the first one hits: a page file belongs
    /// to exactly one tile set, and every page file of that set shares its GTS.
    pub gts_candidates: Vec<PathBuf>,
}

/// Stage the page file `matched` names plus the GTS that covers it into `shared`.
///
/// Staged files are reused by file name: one GTS carries the metadata of a whole tile set and is
/// shared by all of its page files, so re-reading it from the archives for every virtual texture
/// would be wasted work.
pub fn stage_sources(
    pak: &mut Archives,
    matched: &GtpMatch,
    shared: &Path,
) -> Result<StagedSources, String> {
    let gtp = stage_page_file(pak, matched.gtp_path.as_str(), shared)?;
    let gts_candidates = stage_gts_candidates(pak, matched, shared)?;
    Ok(StagedSources {
        gtp,
        gts_candidates,
    })
}

/// Stage the page file at `gtp_rel` into `shared`, reuse the copy already there. Page files are read
/// from the virtual texture archive and nowhere else.
fn stage_page_file(pak: &mut Archives, gtp_rel: &str, shared: &Path) -> Result<PathBuf, String> {
    let path = staged_path(gtp_rel, shared).ok_or_else(|| format!("Invalid GTP path: {gtp_rel}"))?;
    if path.exists() {
        return Ok(path);
    }
    let bytes = pak.read_from(Pak::VirtualTextures, gtp_rel)?;
    fs::write(&path, bytes).map_err(|e| format!("Failed to stage GTP: {e}"))?;
    Ok(path)
}

/// Where a staged copy of `rel` lives inside `shared`; `None` when the archive path has no file name
fn staged_path(rel: &str, shared: &Path) -> Option<PathBuf> {
    let name = Path::new(rel).file_name().and_then(|n| n.to_str())?;
    Some(shared.join(name))
}

/// Resolve GTS candidates and stage them into the `shared` directory, ordered by likelihood:
/// 1. `<GTP with the `_<hash>` suffix stripped>.gts` — the tile set's own GTS (maclarian's standard
///    derivation), the only candidate that is normally needed
/// 2. `.gts` files in the same directory whose name is a prefix of the GTP file name (fallback for
///    mismatched GTS/GTP naming); the longer the GTS name, the higher the priority
///
/// More than one candidate is staged because a GTS outlives the page file that led here — tile sets
/// do not always share the index spelling of their page files — and `infrastructure::export` tries
/// them in order until one accepts the page file. Staged files are reused by file name, so the GTS
/// of a tile set is read once no matter how many of its page files this export touches.
fn stage_gts_candidates(
    pak: &mut Archives,
    matched: &GtpMatch,
    shared: &Path,
) -> Result<Vec<PathBuf>, String> {
    let gtp_rel = matched.gtp_path.as_str();
    let gts_rel = derive_gts_path(gtp_rel);
    let mut staged: Vec<PathBuf> = Vec::new();
    // The derived name is the one that normally hits, and the fallbacks are only worth scanning when
    // it does not
    if !stage_gts_file(pak, &gts_rel, shared, &mut staged) {
        stage_fallbacks(pak, gtp_rel, shared, &mut staged)?;
    }
    finish_candidates(gts_rel, staged)
}

/// The staged candidates, or the error naming the GTS that could not be found
fn finish_candidates(gts_rel: String, staged: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
    if staged.is_empty() {
        return Err(missing_gts_error(&gts_rel));
    }
    Ok(staged)
}

/// What a tile set whose GTS could not be staged reports
fn missing_gts_error(gts_rel: &str) -> String {
    format!(
        "{gts_rel} not found in {}",
        Pak::VirtualTextures.file_name()
    )
}

/// Stage the fallback candidates of one page file: the closest GTS name first, since the closer it is
/// to the GTP name, the more likely it is the one that accepts the page file
fn stage_fallbacks(
    pak: &mut Archives,
    gtp_rel: &str,
    shared: &Path,
    staged: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let mut fallbacks = gts_fallbacks(pak, gtp_rel)?;
    fallbacks.sort_by_key(|rel| std::cmp::Reverse(rel.len()));
    for rel in fallbacks {
        stage_gts_file(pak, &rel, shared, staged);
    }
    Ok(())
}

/// `.gts` files of the virtual texture archive that could cover the page file: in the GTP's own
/// directory, and named as a prefix of the GTP (tile sets do not always share the index spelling of
/// their page files)
fn gts_fallbacks(pak: &mut Archives, gtp_rel: &str) -> Result<Vec<String>, String> {
    let gtp_stem = file_stem_lowercase(gtp_rel);
    let gtp_dir = parent_dir_lowercase(gtp_rel);
    Ok(pak
        .list_in(Pak::VirtualTextures)?
        .into_iter()
        .filter(|rel| is_gts_in_dir(rel, &gtp_dir))
        .filter(|rel| {
            let stem = file_stem_lowercase(rel);
            !stem.is_empty() && gtp_stem.starts_with(&stem)
        })
        .collect())
}

/// Whether `rel` is a `.gts` in `dir`, both already lower case
fn is_gts_in_dir(rel: &str, dir: &str) -> bool {
    rel.to_lowercase().ends_with(".gts") && parent_dir_lowercase(rel) == dir
}

/// File stem of an archive path, lower case; empty when it has none
fn file_stem_lowercase(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Parent directory of an archive path, lower case; empty when it has none
fn parent_dir_lowercase(rel: &str) -> String {
    Path::new(rel)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Stage one GTS to disk; reuse it directly when already staged (shared with another GTP)
fn stage_gts_file(
    pak: &mut Archives,
    rel: &str,
    shared: &Path,
    staged: &mut Vec<PathBuf>,
) -> bool {
    let Some(path) = staged_path(rel, shared) else {
        return false;
    };
    if path.exists() || stage_gts_bytes(pak, rel, &path) {
        staged.push(path);
        return true;
    }
    false
}

/// Write the GTS at `rel` out to `path`; `false` when it cannot be read or written
fn stage_gts_bytes(pak: &mut Archives, rel: &str, path: &Path) -> bool {
    let Ok(bytes) = pak.read_from(Pak::VirtualTextures, rel) else {
        return false;
    };
    fs::write(path, &bytes).is_ok()
}

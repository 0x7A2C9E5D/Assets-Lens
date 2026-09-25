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
    let gtp_rel = matched.gtp_path.as_str();
    let gtp_name = Path::new(gtp_rel)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid GTP path: {gtp_rel}"))?;

    let gtp = shared.join(gtp_name);
    if !gtp.exists() {
        // Page files are read from the virtual texture archive and nowhere else
        let bytes = pak.read_from(Pak::VirtualTextures, gtp_rel)?;
        fs::write(&gtp, bytes).map_err(|e| format!("Failed to stage GTP: {e}"))?;
    }

    let gts_candidates = stage_gts_candidates(pak, matched, shared)?;
    Ok(StagedSources {
        gtp,
        gts_candidates,
    })
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

    let gtp_stem = Path::new(gtp_rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let gtp_dir = Path::new(gtp_rel)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut staged: Vec<PathBuf> = Vec::new();

    // 1. Standard derived name
    if stage_gts_file(pak, &gts_rel, shared, &mut staged) {
        return Ok(staged);
    }

    // 2. Same-directory prefix fallback: the tile set names of the virtual texture archive, filtered
    //    to the GTP's directory and a name prefix
    let mut fallbacks: Vec<String> = pak
        .list_in(Pak::VirtualTextures)?
        .into_iter()
        .filter(|p| {
            p.to_lowercase().ends_with(".gts")
                && Path::new(p)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    == gtp_dir
        })
        .filter(|p| {
            let stem = Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            !stem.is_empty() && gtp_stem.starts_with(&stem)
        })
        .collect();
    // The closer the GTS name is to the GTP name, the more likely it hits; try in descending
    // file-name length
    fallbacks.sort_by_key(|p| std::cmp::Reverse(p.len()));

    for rel in fallbacks {
        stage_gts_file(pak, &rel, shared, &mut staged);
    }

    if staged.is_empty() {
        return Err(format!(
            "{gts_rel} not found in {}",
            Pak::VirtualTextures.file_name()
        ));
    }
    Ok(staged)
}

/// Stage one GTS to disk; reuse it directly when already staged (shared with another GTP)
fn stage_gts_file(pak: &mut Archives, rel: &str, shared: &Path, staged: &mut Vec<PathBuf>) -> bool {
    let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let path = shared.join(name);
    if path.exists() {
        staged.push(path);
        return true;
    }
    match pak.read_from(Pak::VirtualTextures, rel) {
        Ok(bytes) => match fs::write(&path, &bytes) {
            Ok(()) => {
                staged.push(path);
                true
            }
            Err(_) => false,
        },
        Err(_) => false,
    }
}

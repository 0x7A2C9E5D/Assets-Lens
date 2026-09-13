//! Virtual texture page files: stage the page file a `GtpMatch` names plus the GTS that covers it.
//!
//! A GTS is not the companion of one page file: it is the metadata of a whole tile set
//! (`<Base>_<index>.gts`), listing every page file of that set, so one GTS serves many GTPs. The
//! extractor resolves a page file by looking its hash up in the GTS metadata, which is what makes
//! trying several candidates safe — a GTS from another tile set fails instead of exporting the
//! wrong pixels.
//!
//! Both live inside the archives as raw blocks, while `VirtualTextureExtractor` takes file paths, so
//! they are written to a staging directory first. Finding them is archive work rather than export
//! work: this module only produces paths, and `export.rs` turns them into exported artifacts.

use std::fs;
use std::path::{Path, PathBuf};

use maclarian::merged::GtpMatch;

use crate::archives::Archives;

/// Files staged for one page file, both already on disk
pub struct StagedSources {
    /// Page file (`.gtp`), named after its archive entry
    pub gtp: PathBuf,
    /// GTS candidates to try, ordered by likelihood; the same GTS is shared by every page file of
    /// its tile set
    pub gts: Vec<PathBuf>,
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
        // The match names the archive that holds this page file, so it is read straight out of it
        // instead of scanning every archive for a path the lookup already resolved
        let bytes = read_from_match(pak, matched, gtp_rel)?;
        fs::write(&gtp, bytes).map_err(|e| format!("Failed to stage GTP: {e}"))?;
    }

    let gts = gts_candidates(pak, matched, shared)?;
    Ok(StagedSources { gtp, gts })
}

/// GTP path → GTS path: strip the trailing `_<32 hex digits>` and swap the extension
/// (`Generated/Public/VirtualTextures/Albedo_Normal_Physical_5_<hash>.gtp` →
/// `Generated/Public/VirtualTextures/Albedo_Normal_Physical_5.gts`)
///
/// What remains is the tile set index, not a per-file name: every page file of that set (same index,
/// its own hash) derives the same GTS, which is how one GTS comes to serve many GTPs. maclarian
/// derives the name the same way (`virtual_texture/utils.rs::find_gts_path`).
fn derive_gts_path(gtp_path: &str) -> String {
    // The directory is split off first and put back at the end: only the file name loses its hash
    // suffix, while the directory has to survive into the result (a GTS sits beside its page file)
    let (dir, name) = gtp_path.rsplit_once('/').unwrap_or(("", gtp_path));
    let stem = name
        .strip_suffix(".gtp")
        .or_else(|| name.strip_suffix(".GTP"))
        .unwrap_or(name);

    let stripped = stem.rfind('_').filter(|pos| {
        let suffix = &stem[pos + 1..];
        suffix.len() == 32 && suffix.chars().all(|c| c.is_ascii_hexdigit())
    });
    let stem = stripped.map_or(stem, |pos| &stem[..pos]);

    if dir.is_empty() {
        format!("{stem}.gts")
    } else {
        format!("{dir}/{stem}.gts")
    }
}

/// Read a file through the archive a match names. `GtpMatch::pak_path` is the archive its page file
/// was listed from, so reading there is a table lookup rather than a scan; the pool-wide read stays
/// as the fallback for names the match does not pin down (`GtpMatch` covers the GTP only — the GTS
/// for its tile set is derived from the page file name).
fn read_from_match(pak: &mut Archives, matched: &GtpMatch, target: &str) -> Result<Vec<u8>, String> {
    pak.read_in(matched.pak_path.as_path(), target)
        .or_else(|_| pak.read(target, None))
}

/// Resolve GTS candidates and stage them into the `shared` directory, ordered by likelihood:
/// 1. `<GTP with the `_<hash>` suffix stripped>.gts` — the tile set's own GTS (maclarian's standard
///    derivation), the only candidate that is normally needed
/// 2. `.gts` files in the same directory whose name is a prefix of the GTP file name (fallback for
///    mismatched GTS/GTP naming); the longer the GTS name, the higher the priority
///
/// More than one candidate is staged because a GTS outlives the page file that led here — tile sets
/// do not always share the index spelling of their page files — and `export.rs` tries them in order
/// until one accepts the page file. Staged files are reused by file name, so the GTS of a tile set is
/// read once no matter how many of its page files this export touches. `matched` gives the archive to
/// try first; the pool-wide scan remains for the derived names it does not cover.
fn gts_candidates(
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
    if stage_gts_file(pak, matched, &gts_rel, shared, &mut staged) {
        return Ok(staged);
    }

    // 2. Same-directory prefix fallback: filter GTS files sharing the directory and a name prefix
    //    across all PAKs
    let fallbacks: Vec<String> = pak
        .list_all()?
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
    let mut fallbacks_sorted = fallbacks;
    fallbacks_sorted.sort_by_key(|p| std::cmp::Reverse(p.len()));

    for rel in fallbacks_sorted {
        stage_gts_file(pak, matched, &rel, shared, &mut staged);
    }

    if staged.is_empty() {
        return Err(format!("{gts_rel} not found in any PAK"));
    }
    Ok(staged)
}

/// Stage one GTS to disk; reuse it directly when already staged (shared with another GTP)
fn stage_gts_file(
    pak: &mut Archives,
    matched: &GtpMatch,
    rel: &str,
    shared: &Path,
    staged: &mut Vec<PathBuf>,
) -> bool {
    let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let path = shared.join(name);
    if path.exists() {
        staged.push(path);
        return true;
    }
    match read_from_match(pak, matched, rel) {
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

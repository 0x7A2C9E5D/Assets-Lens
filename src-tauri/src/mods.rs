//! Discovery of player-installed mod archives.
//!
//! BG3 keeps mods outside the game installation, as loose `.pak` files in the per-user profile
//! directory. They use the same LSPK container as the game's own archives, so reading them needs no
//! new code — only the location differs, which is what this module resolves.

use std::fs;
use std::path::PathBuf;

use crate::export::is_data_partition;

/// Per-user data root, i.e. the directory holding `Larian Studios`. `std::env` rather than a Tauri
/// path API because this runs inside `spawn_blocking`, where no `AppHandle` is available.
fn user_data_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }

    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library").join("Application Support"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
    }
}

/// The directory holding installed mod archives; `None` when the profile does not exist (the game
/// or the in-game mod manager has never been started), so callers can treat "no mods" uniformly.
pub fn mods_dir() -> Option<PathBuf> {
    let dir = user_data_root()?
        .join("Larian Studios")
        .join("Baldur's Gate 3")
        .join("Mods");
    dir.is_dir().then_some(dir)
}

/// Every mod archive, sorted by file name so the merge order stays identical across runs.
///
/// Only flat `*.pak` files are listed: that is how BG3's own mod manager installs them. The real
/// load order lives in `modsettings.lsx`, which is deliberately not parsed here — file-name order
/// is a documented approximation, and it never affects whether a mod is scanned, only which of two
/// mods wins a path they both ship. Numbered data partitions (`<Name>_<n>.pak`) are excluded like
/// in the game directory: they carry no LSPK header of their own.
pub fn list_mod_paks() -> Vec<PathBuf> {
    let Some(dir) = mods_dir() else {
        return Vec::new();
    };

    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut paks: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pak"))
        })
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_data_partition)
        })
        .collect();

    paks.sort();
    paks
}

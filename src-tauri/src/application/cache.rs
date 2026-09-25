//! The built database, written to disk so the next launch reads it instead of scanning the game's
//! paks again (a full build runs for minutes).
//!
//! One file per game data directory, named after the directory itself (see `digest`): several
//! installations can each keep their index, and switching between them costs no scan. A file is only used
//! while it still matches the sources it was built from — the app version, the game's own paks and
//! the mod paks, each by size and modification time — so a changed source is reported (see
//! `domain::app::CacheStatus`) rather than silently answered with a stale index.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use maclarian::merged::MergedDatabase;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::application::state::{default_mods_path, mod_paks};
use crate::domain::material::MaterialInfo;
use crate::domain::source::ModSources;

/// Bumped whenever the written shape or what a build puts in it changes: a file from another format
/// version is refused instead of misread.
const FORMAT_VERSION: u32 = 1;

/// The game's own paks, in the order a build reads them (see `commands::build_database`)
const GAME_PAKS: [&str; 2] = ["Shared.pak", "GustavX.pak"];

/// Stable reasons a persisted database was refused; the frontend maps them to copy
const CODE_VERSION: &str = "version";
const CODE_GAME_PAKS: &str = "game_paks";
const CODE_MOD_PAKS: &str = "mod_paks";
const CODE_UNREADABLE: &str = "unreadable";

/// One source file of a build, as it was when the build ran.
///
/// The raw fields rather than a hash of them: `DefaultHasher` promises nothing about its output
/// across Rust versions, and a file name that changes with the toolchain would silently orphan every
/// cache on the disk.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PakStamp {
    /// File name — the paks are compared as a name-ordered list
    pub name: String,
    pub size: u64,
    /// Unix seconds; 0 when the file system will not say
    pub modified: u64,
}

impl PakStamp {
    /// The stamp of one file. `None` when it cannot be read, which is also how an absent pak shows
    /// up: the two are indistinguishable in a set of stamps, and both mean the same thing — the file
    /// the build read is no longer there.
    fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs());
        Some(Self {
            name: path.file_name()?.to_string_lossy().into_owned(),
            size: metadata.len(),
            modified,
        })
    }
}

/// The cache file, as read back. `PersistedDatabaseRef` is the same file as written — the two must
/// stay in step, which is why each carries only to derive its own direction needs.
#[derive(Deserialize)]
pub struct PersistedDatabase {
    /// See `FORMAT_VERSION`
    pub format_version: u32,
    /// The app version that wrote the file (`CARGO_PKG_VERSION`)
    pub app_version: String,
    /// Unix seconds the build finished
    pub built_at: u64,
    pub game_paks: Vec<PakStamp>,
    pub mod_paks: Vec<PakStamp>,
    pub database: MergedDatabase,
    pub materials: HashMap<String, MaterialInfo>,
    pub mod_sources: ModSources,
}

/// The cache file, as written. Borrowed: the build still owns these values when the file is saved,
/// and cloning a database of several hundred thousand entries just to write it out is not worth it.
#[derive(Serialize)]
struct PersistedDatabaseRef<'a> {
    format_version: u32,
    app_version: &'a str,
    built_at: u64,
    game_paks: &'a [PakStamp],
    mod_paks: &'a [PakStamp],
    database: &'a MergedDatabase,
    materials: &'a HashMap<String, MaterialInfo>,
    mod_sources: &'a ModSources,
}

/// What `load` found for a game directory
pub enum Loaded {
    /// A matching index: ready to be published into the state
    Cache(Box<PersistedDatabase>),
    /// Nothing was ever written for this directory
    Missing,
    /// A file is there but cannot be used; `code` is one of the `CODE_*` values above
    Stale {
        code: &'static str,
        detail: Option<String>,
    },
}

/// The file holding one game directory's persisted database: one name per directory inside the app's
/// data directory (`%APPDATA%\com.bg3.assets-lens` on Windows).
pub fn cache_file(app: &AppHandle, game_path: &Path) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("App data directory unavailable: {err}"))?;
    Ok(dir.join(format!("database-{}.json", digest(&normalize(game_path)))))
}

/// Write the index built for `game_path`, replacing whatever was there.
///
/// Taken by reference and called with the state lock released: serializing a 20 MB tree is not
/// something to do while every other command waits for the lock. A failed write is the caller's to
/// report — the build that produced the index has succeeded either way.
pub fn save(
    app: &AppHandle,
    game_path: &Path,
    database: &MergedDatabase,
    materials: &HashMap<String, MaterialInfo>,
    mod_sources: &ModSources,
) -> Result<(), String> {
    let file = cache_file(app, game_path)?;
    ensure_parent(&file)?;
    let (game_paks, mod_paks) = stamps(game_path);
    let payload = payload(database, materials, mod_sources, (&game_paks, &mod_paks));
    write_atomic(&file, &payload)
}

/// The written shape of one build, borrowed from the values the build still owns.
///
/// Over the line limit deliberately: this is one struct literal, and moving any field out would only
/// separate it from the name it states without shortening anything.
fn payload<'a>(
    database: &'a MergedDatabase,
    materials: &'a HashMap<String, MaterialInfo>,
    mod_sources: &'a ModSources,
    (game_paks, mod_paks): (&'a [PakStamp], &'a [PakStamp]),
) -> PersistedDatabaseRef<'a> {
    PersistedDatabaseRef {
        format_version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        built_at: now(),
        game_paks,
        mod_paks,
        database,
        materials,
        mod_sources,
    }
}

/// Read the index persisted for `game_path`, if one matches the sources as they are now.
pub fn load(app: &AppHandle, game_path: &Path) -> Loaded {
    let bytes = match read_bytes(app, game_path) {
        Ok(bytes) => bytes,
        Err(loaded) => return loaded,
    };

    let persisted: PersistedDatabase = match serde_json::from_slice(&bytes) {
        Ok(persisted) => persisted,
        Err(err) => return stale(CODE_UNREADABLE, Some(err.to_string())),
    };

    if !written_by_this_build(&persisted) {
        return stale(CODE_VERSION, Some(persisted.app_version));
    }

    verify_sources(persisted, game_path)
}

/// The cache file's bytes, or the answer to give instead: a file that is not there at all is the
/// ordinary first-run case, not a reason to report anything.
fn read_bytes(app: &AppHandle, game_path: &Path) -> Result<Vec<u8>, Loaded> {
    let file = match cache_file(app, game_path) {
        Ok(file) => file,
        Err(err) => return Err(stale(CODE_UNREADABLE, Some(err))),
    };

    match fs::read(&file) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(Loaded::Missing),
        Err(err) => Err(stale(
            CODE_UNREADABLE,
            Some(format!("{}: {err}", file.display())),
        )),
    }
}

/// Whether the file was written by this very build. One test for both: a format version only changes
/// alongside an app version, and the version that wrote the file is what the page needs to name
/// either way.
fn written_by_this_build(persisted: &PersistedDatabase) -> bool {
    persisted.format_version == FORMAT_VERSION && persisted.app_version == env!("CARGO_PKG_VERSION")
}

/// Check the file against the sources as they are now, and the one invariant worth checking on the way
/// back in: an empty index would look like a built database to every command that asks for one, and
/// the page would report zero of everything.
fn verify_sources(persisted: PersistedDatabase, game_path: &Path) -> Loaded {
    let (game_paks, mod_paks) = stamps(game_path);
    if persisted.game_paks != game_paks {
        return stale(CODE_GAME_PAKS, None);
    }
    if persisted.mod_paks != mod_paks {
        return stale(CODE_MOD_PAKS, None);
    }
    if persisted.database.visuals_by_id.is_empty() {
        return stale(
            CODE_UNREADABLE,
            Some("the persisted index holds no visual".into()),
        );
    }

    Loaded::Cache(Box::new(persisted))
}

/// The stamps of everything a build of `game_path` reads: the game's own paks and the mod paks.
///
/// The mod list comes from the same two helpers the archive pool is built from (see
/// `AppState::pool`), so a fingerprint can only differ when the build's own input did.
fn stamps(game_path: &Path) -> (Vec<PakStamp>, Vec<PakStamp>) {
    let game = GAME_PAKS
        .iter()
        .filter_map(|name| PakStamp::of(&game_path.join(name)))
        .collect();

    let mods = default_mods_path()
        .map(|dir| mod_paks(&dir))
        .unwrap_or_default()
        .iter()
        .filter_map(|path| PakStamp::of(path))
        .collect();

    (game, mods)
}

/// Serialize `value` into `path` and flush it all the way to the platter, so the rename that follows
/// cannot publish a file whose contents are still sitting in a buffer.
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let describe = |err: &dyn std::fmt::Display| format!("{}: {err}", path.display());

    let file = File::create(path).map_err(|err| describe(&err))?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, value).map_err(|err| describe(&err))?;
    writer.flush().map_err(|err| describe(&err))?;
    let file = writer.into_inner().map_err(|err| describe(&err))?;
    file.sync_all().map_err(|err| describe(&err))
}

/// Write `value` to `file` beside it and rename it over the target: a full disk or a process killed
/// mid-write then leaves the previous file untouched, instead of a truncated one that reads as
/// corrupt.
///
/// Shared with `settings`, which writes its own small file the same way rather than keeping a second
/// copy of the pattern.
pub(crate) fn write_atomic(file: &Path, value: &impl Serialize) -> Result<(), String> {
    let temp = file.with_extension("json.tmp");
    let saved = write_json(&temp, value)
        .and_then(|()| fs::rename(&temp, file).map_err(|err| format!("{}: {err}", file.display())));
    if saved.is_err() {
        // A half-written file must not be left lying around for the next launch to trip over
        let _ = fs::remove_file(&temp);
    }
    saved
}

/// Make sure the directory holding `file` exists, so to write beside it can land
pub(crate) fn ensure_parent(file: &Path) -> Result<(), String> {
    match file.parent() {
        Some(dir) => fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display())),
        None => Ok(()),
    }
}

/// A directory as a stable string: lowercased, `/` separators, no trailing one.
///
/// Windows paths are case-insensitive and the frontend hands back the text the user chose, so the
/// same directory can arrive spelled differently (`D:\GOG Games\…` and `D:/gog games/…`). Normalizing
/// before hashing keeps both spellings on one cache file.
fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// FNV-1a over the string's bytes, as 16 lowercase hex digits.
///
/// Written out rather than taken from `std::hash`: a cache file name has to keep meaning the same
/// directory across builds, which a hasher free to change its algorithm does not promise.
fn digest(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Unix seconds, or 0 when the clock reads before the epoch — a stamp nothing can match, so such a
/// file is rebuilt rather than trusted
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// A refused cache file, with the reason the page reports
fn stale(code: &'static str, detail: Option<String>) -> Loaded {
    Loaded::Stale { code, detail }
}

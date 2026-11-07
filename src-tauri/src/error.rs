//! Errors crossing the Tauri command boundary.
//!
//! Every variant states what failed and where. `Display` renders the exact text the frontend
//! matches on, and `Serialize` keeps the wire format a plain string.

use serde::ser::{Serialize, Serializer};

/// Error returned by commands and the modules they call, serialized as its `Display` output
/// (`#[specta(type = String)]` mirrors that); the messages below must stay verbatim.
#[derive(Debug, thiserror::Error, specta::Type)]
#[specta(type = String)]
pub enum AppError {
    /// No game data directory chosen yet, while the command needs one
    #[error("BG3 Data directory is not set. Please use auto-detect or select a directory first.")]
    NotConfigured,
    /// Directory chosen, but the resource database has not been built
    #[error(
        "Resource database has not been built. Please go to the Database page and build it first."
    )]
    NotBuilt,
    /// `set_game_path` was given a path that does not exist
    #[error("Directory does not exist: {path}")]
    DirectoryMissing { path: String },
    /// `set_game_path` was given a directory without `Shared.pak`
    #[error(
        "Shared.pak not found in this directory. Please select the BG3 Data directory: {path}"
    )]
    SharedPakMissing { path: String },
    /// maclarian's resolver could not be created for the directory
    #[error("Failed to initialize resource parser: {detail}")]
    ResolverInit { detail: String },
    /// The directory was switched while the database was being parsed
    #[error("Game data directory changed while building; the result was discarded.")]
    GameDirectoryChanged,
    /// A `spawn_blocking` task panicked or was cancelled; `task` is the noun the message starts with
    #[error("{task} task terminated unexpectedly: {detail}")]
    TaskTerminated { task: &'static str, detail: String },
    /// The export target directory does not exist
    #[error("Export directory does not exist: {path}")]
    ExportDirectoryMissing { path: String },
    /// The shared state mutex was poisoned
    #[error("State lock unavailable: {detail}")]
    StateLock { detail: String },
    /// The PAK read pool mutex was poisoned
    #[error("PAK pool lock unavailable: {detail}")]
    ArchivePoolLock { detail: String },
    /// An archive file could not be opened
    #[error("Failed to open {path}: {detail}")]
    ArchiveOpen { path: String, detail: String },
    /// An opened archive's file table could not be read
    #[error("Failed to read file table of {path}: {detail}")]
    ArchiveFileTable { path: String, detail: String },
    /// The named file is not in the archive the caller expected
    #[error("{target} not found in {pak}")]
    ArchiveEntryMissing { target: String, pak: &'static str },
    /// The archive's reader went missing after its table was cached
    #[error("Reader unavailable for {pak}")]
    ArchiveReaderMissing { pak: &'static str },
    /// An archive entry could not be decompressed
    #[error("Failed to decompress {target}: {detail}")]
    ArchiveDecompress { target: String, detail: String },
    /// A directory could not be created; `target` names what it is for (`export`, `textures`, …)
    #[error("Failed to create {target} directory: {detail}")]
    CreateDirectory {
        target: &'static str,
        detail: String,
    },
    /// A virtual texture staging directory could not be recreated between GTS candidates
    #[error("Failed to recreate staging directory: {detail}")]
    RecreateStagingDirectory { detail: String },
    /// The mesh file could not be written
    #[error("Failed to write mesh ({format}): {detail}")]
    MeshWrite {
        format: &'static str,
        detail: String,
    },
    /// The mesh bytes could not be read out of `Models.pak`
    #[error("Mesh data unavailable: {detail}")]
    MeshUnavailable { detail: String },
    /// GR2 → GLB conversion failed during an export (a preview surfaces the raw message instead)
    #[error("Failed to convert mesh to GLB: {detail}")]
    MeshConvert { detail: String },
    /// No GTS candidate was compatible with the page file
    #[error("Virtual texture extraction failed: {detail}")]
    VirtualTextureExtraction { detail: String },
    /// An extracted layer could not be moved into the export directory
    #[error("Failed to move {layer} output: {detail}")]
    MoveLayerOutput { layer: &'static str, detail: String },
    /// The extractor output directory could not be listed
    #[error("Failed to list extraction output: {detail}")]
    ListExtractionOutput { detail: String },
    /// A message that has to reach the frontend untouched, or comes from a module that still
    /// returns plain strings
    #[error("{0}")]
    Message(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

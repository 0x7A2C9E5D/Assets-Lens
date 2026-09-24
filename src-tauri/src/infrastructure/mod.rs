//! The outside world: the game's archives, the files this app reads and writes, and the mods it
//! folds into the index.
//!
//! Everything here needs what the process does not own itself — a PAK on disk, the app data
//! directory, the mods folder — so it sits outside `domain` and is depended on *by* the layers above
//! rather than the other way round. One deliberate exception: the modules that parse a game format
//! (`domain::virtual_textures`, `domain::virtual_texture_params`) cannot do that without reading the
//! archives, so they reach into `archives` (see `domain` for why that narrowing was accepted).
//!
//! The four modules:
//!
//! 1. `archives` — the PAK read pool: the game's four archives plus any mod archive, each opened once
//!    and kept resident;
//! 2. `cache` — the built database written to disk, so the next launch reads it back instead of
//!    scanning the paks again;
//! 3. `mods` — a mod's banks read into the index and merged over the game's own resources by GUID;
//! 4. `export` — the export pipeline: reads through the pool above, writes the artifacts, and
//!    assembles the manifest.
//!
//! None of them is a command handler: the shapes they take in and hand back are `domain`'s, and the
//! only Tauri-aware piece is `cache`, which takes an `AppHandle` to reach the app data directory.

pub mod archives;
pub(crate) mod cache;
pub mod export;
pub mod mods;
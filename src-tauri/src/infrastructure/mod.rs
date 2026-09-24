//! The outside world: the game's archives, and the mods it folds into the index.
//!
//! Everything here needs what the process does not own itself — a PAK on disk, the mods folder — so it
//! sits outside `domain` and is depended on *by* the layers above rather than the other way round. One
//! deliberate exception: the modules that parse a game format (`domain::virtual_textures`,
//! `domain::virtual_texture_params`) cannot do that without reading the archives, so they reach into
//! `archives` (see `domain` for why that narrowing was accepted).
//!
//! The three modules:
//!
//! 1. `archives` — the PAK read pool: the game's four archives plus any mod archive, each opened once
//!    and kept resident;
//! 2. `mods` — a mod's banks read into the index and merged over the game's own resources by GUID;
//! 3. `export` — the export pipeline: reads through the pool above, writes the artifacts, and
//!    assembles the manifest.
//!
//! None of them is a command handler or knows about the app's own state: the shapes they take in and
//! hand back are `domain`'s, and what the app remembers between launches lives in `application`.

pub mod archives;
pub mod export;
pub mod mods;

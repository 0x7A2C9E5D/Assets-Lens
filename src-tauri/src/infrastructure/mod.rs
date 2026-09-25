//! The outside world: the game's archives, and the mods it folds into the index.
//!
//! Everything here needs what the process does not own itself — a PAK on disk, the mods folder — so it
//! sits outside `domain` and is depended on by the layers above rather than the other way round. One
//! deliberate exception: `domain::virtual_textures` and `domain::virtual_texture_params` cannot parse a
//! format without reading the archives, so they reach into `archives`.
//!
//! `archives` is the PAK read pool, `mods` reads a mod's banks and merges them over the game's own
//! resources by GUID, and `export` is the export pipeline. None of them is a command handler or knows
//! about the app's own state.

pub mod archives;
pub mod export;
pub mod mods;

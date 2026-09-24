//! The application's own model and the rules that go with it.
//!
//! Two kinds of things live here, and both are ours:
//!
//! 1. plain structures and pure functions — the sources a resource comes from, the material cache,
//!    the naming rules (`source`, `material`, `naming`);
//! 2. what this app defines for the game data it reads — the asset model and its serialization
//!    (`models`), the tile set layout behind a virtual texture page file (`virtual_textures`), and
//!    the parameter names of a material template (`virtual_texture_params`).
//!
//! The game's formats are this application's subject matter, so reading them is model work: the two
//! format modules do use the file system and maclarian types. That is a deliberate narrowing of the
//! earlier "no file system, no maclarian type" rule, which now holds only for the modules that are
//! purely about our own concepts.
//!
//! Still missing from the layer is the split of the files left flat — the runtime state and the
//! command handlers (`state.rs`, `commands.rs`), the archive / cache / mod plumbing (`archives.rs`,
//! `cache.rs`, `mods.rs`) and the export workflow (`export.rs`) each mix `infrastructure`,
//! `application` and `ipc` concerns.

pub mod material;
pub mod models;
pub mod naming;
pub mod source;
pub mod virtual_texture_params;
pub mod virtual_textures;
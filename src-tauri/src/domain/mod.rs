//! The application's own model and the rules that go with it.
//!
//! Two kinds of things live here, and both are ours:
//!
//! 1. plain structures and pure functions — the sources a resource comes from, the material cache,
//!    the naming rules (`source`, `material`, `naming`);
//! 2. what this app defines for the game data it reads — the rows of an asset and how they are
//!    assembled (`visual`, `texture`, `material`, `virtual_textures`), the parameter names of a
//!    material template (`virtual_texture_params`), and the shapes of an export and of the app's own
//!    state (`export`, `app`).
//!
//! The game's formats are this application's subject matter, so working with them is model work: the
//! modules that parse one (`virtual_textures`, `virtual_texture_params`) do use the file system, and
//! the row builders take maclarian's own types. That is a deliberate narrowing of the earlier "no
//! file system, no maclarian type" rule, which now holds only for `source` and `naming`.
//!
//! Still missing from the layer is the split of the files left flat — the runtime state and the
//! command handlers (`state.rs`, `commands.rs`), the archive / cache / mod plumbing (`archives.rs`,
//! `cache.rs`, `mods.rs`) and the export workflow (`export.rs`) each mix `infrastructure`,
//! `application` and `ipc` concerns.

pub mod app;
pub mod export;
pub mod material;
pub mod naming;
pub mod source;
pub mod texture;
pub mod virtual_texture_params;
pub mod virtual_textures;
pub mod visual;

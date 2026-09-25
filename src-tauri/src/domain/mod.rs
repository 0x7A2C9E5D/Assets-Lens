//! The application's own model and the rules that go with it.
//!
//! Plain structures and pure functions for the sources a resource comes from, the material cache and
//! the naming rules (`source`, `material`, `naming`), plus the rows this app defines for the game data
//! it reads (`visual`, `texture`, `export`, `app`, `virtual_textures`, `virtual_texture_params`).
//!
//! The game's formats are this app's subject, so `virtual_textures` and `virtual_texture_params` do
//! read archives and take maclarian's types; the "no file system, no maclarian type" rule now holds
//! only for `source` and `naming`. Archive plumbing and the export pipeline live in `infrastructure`.

pub mod app;
pub mod export;
pub mod material;
pub mod naming;
pub mod source;
pub mod texture;
pub mod virtual_texture_params;
pub mod virtual_textures;
pub mod visual;

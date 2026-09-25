//! What the app knows about itself, and the commands that act on it.
//!
//! Everything here is the application's own bookkeeping: the built index held in memory for the session
//! (`state`) and written to disk so the next launch reads it back instead of scanning the packs again
//! (`cache`), the game directory recorded so the next launch comes back to it (`settings`), and the
//! handlers the frontend calls (`commands`).
//!
//! `cache` and `settings` write files of their own through one atomic writer, so a write cut short
//! cannot be published; `commands` is the `ipc` edge, where Tauri's attributes sit in the module that
//! orchestrates the work instead of in a layer of their own.

pub(crate) mod cache;
pub(crate) mod commands;
pub mod settings;
pub mod state;

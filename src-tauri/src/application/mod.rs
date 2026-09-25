//! What the app knows about itself, and the commands that act on it: the state of the current session,
//! what it keeps between launches, and the Tauri handlers that expose both to the frontend.
//!
//! Unlike `domain` (the game's data as this app models it) and `infrastructure` (the game's files and
//! the rest of the outside world), everything here is the application's own bookkeeping — the built
//! index held in memory for the session (`state`) and written to disk so the next launch reads it back
//! instead of scanning the packs again (`cache`), the game directory recorded so the next launch comes
//! back to it (`settings`), and the handlers the frontend calls (`commands`).
//!
//! Two things in here lean elsewhere and are kept beside the state they serve rather than split out:
//! `cache` and `settings` write files of their own through one atomic writer, so a write cut short
//! cannot be published — `infrastructure`'s kind of work; and `commands` is the `ipc` edge, where
//! Tauri's `#[tauri::command]`, its arguments and its return shapes sit in the module that orchestrates
//! the work instead of in a layer of their own.

pub(crate) mod cache;
pub(crate) mod commands;
pub mod settings;
pub mod state;

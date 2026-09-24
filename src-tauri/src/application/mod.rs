//! What the app knows about itself: the state of the current session, and what it keeps between
//! launches.
//!
//! Unlike `domain` (the game's data as this app models it) and `infrastructure` (the game's files and
//! the rest of the outside world), everything here is the application's own bookkeeping — the built
//! index held in memory for the session (`state`) and written to disk so the next launch reads it back
//! instead of scanning the paks again (`cache`), and the game directory recorded so the next launch
//! comes back to it (`settings`).
//!
//! Two things in here lean towards `infrastructure` and are kept beside the state they serve instead
//! of being split out: `cache` and `settings` write files of their own (through one atomic writer, so
//! a write cut short cannot be published), and `state` carries the mods directory lookup and the
//! material-name workaround around maclarian's crate-private types. All three are about the state
//! they read and fill, which is why they live here.

pub(crate) mod cache;
pub mod settings;
pub mod state;

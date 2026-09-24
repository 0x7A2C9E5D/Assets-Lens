//! What the app knows about itself: the state of the current session, and the little it remembers
//! between launches.
//!
//! Unlike `domain` (the game's data as this app models it) and `infrastructure` (the game's files and
//! the rest of the outside world), everything here is the application's own bookkeeping — the built
//! index held in memory for the session (`state`) and the game directory recorded on disk so the next
//! launch comes back to it (`settings`).
//!
//! Two things in here lean towards `infrastructure` and are kept beside the state they serve instead
//! of being split out: `settings` writes its own file (through the same atomic writer the index cache
//! uses), and `state` carries the mods directory lookup and the material-name workaround around
//! maclarian's crate-private types. Both are about the state they fill, which is why they live here.

pub mod settings;
pub mod state;

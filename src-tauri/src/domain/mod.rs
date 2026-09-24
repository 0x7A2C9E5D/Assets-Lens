//! The application's own concepts and rules, kept free of everything that is not ours.
//!
//! Three things hold for every item below, and they are the whole point of the layer:
//!
//! 1. it is ours — not maclarian's, not the game's archive layout;
//! 2. it is a plain structure or a pure function: no file system, no archive reads, no `AppHandle`;
//! 3. no maclarian type appears in its signature.
//!
//! Being pure is not enough on its own: `parse_page_file_sizes`, `bank_dir` and `find_entry` are pure
//! too, but they encode the layout of an external format, so they stay where they are.
//!
//! The rest of the crate is still flat — `infrastructure`, `application` and `ipc` do not exist yet.
//! What is left behind in `state.rs`, `models.rs`, `mods.rs` and `export.rs` is a mix of those three
//! concerns, and pulling them apart is the next round's work. Cheaper first, since it unblocks the
//! most: an anti-corruption layer at the maclarian boundary, which is what would let the merge and
//! parsing rules of `mods.rs` move in here as well.

pub mod material;
pub mod naming;
pub mod source;
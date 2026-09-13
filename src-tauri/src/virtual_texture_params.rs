//! Virtual texture parameter names, read straight out of the LSF data.
//!
//! maclarian parses a `VirtualTextureParameters` node down to its `ID` — `MaterialDef` carries a list
//! of GUIDs — so the parameter a binding fills (`virtualtexture`, `overlayvirtualtexture`, …) never
//! reaches the app. The name sits in the very node being parsed, so it is read here instead: one
//! pass over every `_merged.lsf` of `Shared.pak`, split over a few threads because the shipped data
//! spreads 7423 bindings over 2620 files that are mostly a few kilobytes each.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use maclarian::formats::lsf::parse_lsf_bytes;

use crate::archives::Archives;
use crate::state::VirtualTextureBinding;

/// How many archive pools one pass may open. Reading is decompression on tiny files, so the work is
/// split over a handful of threads; the cap keeps a many-core machine from opening a pool per core
/// for no measurable gain.
const MAX_WORKERS: usize = 8;

/// The virtual textures every material binds, keyed by material GUID, with the parameter each
/// binding fills.
///
/// The key is the material rather than the virtual texture: the shipped data binds 4742 distinct
/// resources 7423 times, and a resource reached from two materials can fill a different parameter in
/// each (`virtualtexture` on the body, `overlayvirtualtexture` on the overlay that shares its mask).
/// Keying by resource would let one of those names overwrite the other, with the winner decided by
/// whichever thread parsed last.
///
/// An unreadable file, archive or document is skipped: the names decorate a row, and a partial list
/// still beats a build that fails over one file.
pub fn collect(
    pool: &Mutex<Archives>,
    pak: &Path,
) -> HashMap<String, Vec<VirtualTextureBinding>> {
    let paths = match lock_paths(pool, pak) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("[maclarian] virtual texture parameters unavailable: {err}");
            return HashMap::new();
        }
    };

    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_WORKERS);
    let chunk = paths.len().div_ceil(workers).max(1);

    let Some(game_path) = pak.parent() else {
        return HashMap::new();
    };

    let mut materials = HashMap::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|slice| scope.spawn(|| read_parameters(game_path, pak, slice)))
            .collect();

        for handle in handles {
            match handle.join() {
                Ok(part) => {
                    for (material_id, bindings) in part {
                        materials.entry(material_id).or_insert(bindings);
                    }
                }
                Err(_) => eprintln!("[maclarian] a virtual texture parameter pass panicked"),
            }
        }
    });

    materials
}

/// Every `_merged.lsf` inside `pak` — the same filter maclarian builds its database from, so both
/// ends cover the same documents.
fn lock_paths(pool: &Mutex<Archives>, pak: &Path) -> Result<Vec<String>, String> {
    let mut pool = pool
        .lock()
        .map_err(|err| format!("PAK pool lock unavailable: {err}"))?;

    Ok(pool
        .list(pak)?
        .into_iter()
        .filter(|path| path.ends_with("_merged.lsf"))
        .collect())
}

/// One worker: its own archive pool (every read needs `&mut`, so pools cannot be shared), then a
/// parse per file. Only the `MaterialBank` region is walked — bindings never appear elsewhere.
fn read_parameters(
    game_path: &Path,
    pak: &Path,
    paths: &[String],
) -> HashMap<String, Vec<VirtualTextureBinding>> {
    // No `prefer` order: the archive is named explicitly by every read below
    let Ok(mut pool) = Archives::new(game_path, &[]) else {
        return HashMap::new();
    };

    let mut materials = HashMap::new();
    for path in paths {
        let Ok(bytes) = pool.read_in(pak, path) else {
            continue;
        };
        let Ok(doc) = parse_lsf_bytes(&bytes) else {
            continue;
        };

        for root in doc.root_nodes() {
            if doc.node_name(root) != Some("MaterialBank") {
                continue;
            }
            for resource in doc.find_children_by_name(root, "Resource") {
                let bindings: Vec<VirtualTextureBinding> =
                    doc.find_children_by_name(resource, "VirtualTextureParameters")
                        .into_iter()
                        .filter_map(|binding| {
                            let id = doc.get_fixed_string_attr(binding, "ID")?;
                            Some(VirtualTextureBinding {
                                id,
                                // A binding without a name still marks its resource as parameterized
                                parameter_name: doc
                                    .get_fixed_string_attr(binding, "ParameterName")
                                    .unwrap_or_default(),
                            })
                        })
                        .collect();

                if bindings.is_empty() {
                    continue;
                }
                if let Some(material_id) = doc.get_fixed_string_attr(resource, "ID") {
                    materials.insert(material_id, bindings);
                }
            }
        }
    }

    materials
}

//! The commands that build the merged resource database: parsing the game's packs and merging the mods
//! over them, plus the helpers that feed it — the progress split, the visual id orders, and the
//! lazy read of a material's virtual texture parameter names.

use std::path::PathBuf;

use maclarian::merged::MergedDatabase;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::{lock, SharedState, NOT_CONFIGURED};
use crate::application::cache;
use crate::application::state::extract_materials;
use crate::domain::app::{BuildProgress, CacheStatus, DatabaseStats};
use crate::domain::material::fill_virtual_texture_parameters;
use crate::domain::source::ModSources;
use crate::domain::virtual_texture_params;
use crate::infrastructure::archives::lock_pool;
use crate::infrastructure::mods;

/// Share of the build progress the mods are given, when there are any. The game data is one huge
/// archive and the mods are a handful of files, so the split is fixed rather than measured: it keeps
/// the bar from jumping back once the game data is done.
const MOD_WEIGHT: f32 = 0.2;

/// How far the archive has been parsed, as a fraction (0 while it reports no file count)
pub(super) fn parse_fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

/// Every visual GUID in the ascending orders the list can be sorted by: by (name, id) — the default —
/// by id, and by (source, name, id). Names and sources are only sort keys: neither is unique, so
/// using them as the identity would collapse same-keyed visuals into one list row. Sorting by name
/// first keeps those duplicates adjacent while the id tiebreak keeps pagination deterministic. The
/// source order puts the base game first (its records carry no source) and then the mods by name.
///
/// All orders are built here, once, so a sort change only picks a different cached sequence instead
/// of re-sorting every id on each page request.
pub(crate) fn sorted_visual_ids(
    db: &MergedDatabase,
    sources: &ModSources,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut rows: Vec<(&str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| (visual.name.as_str(), visual.id.as_str()))
        .collect();
    rows.sort_unstable();
    let by_name: Vec<String> = rows.into_iter().map(|(_, id)| id.to_string()).collect();

    let mut ids: Vec<&str> = db.visuals_by_id.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let by_id: Vec<String> = ids.into_iter().map(str::to_string).collect();

    let mut sourced: Vec<(&str, &str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| {
            (
                sources
                    .get(&visual.id)
                    .map(String::as_str)
                    .unwrap_or_default(),
                visual.name.as_str(),
                visual.id.as_str(),
            )
        })
        .collect();
    sourced.sort_unstable();
    let by_source: Vec<String> = sourced
        .into_iter()
        .map(|(_, _, id)| id.to_string())
        .collect();

    (by_name, by_id, by_source)
}

/// Build the _merged resource database; progress is pushed through a Channel
#[tauri::command]
pub async fn build_database(
    app: AppHandle,
    on_progress: Channel<BuildProgress>,
) -> Result<DatabaseStats, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<DatabaseStats, String> {
        // Take what the build needs, then release the lock immediately: parsing the game's packs runs
        // for minutes, and every other command needs that lock to answer. The pool is taken here too,
        // so it is built from the mod directory this build belongs to.
        let (resolver, game_path, pool) = {
            let mut st = lock(&state)?;
            (
                st.resolver
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
                st.game_path
                    .clone()
                    .ok_or_else(|| NOT_CONFIGURED.to_string())?,
                st.pool()?,
            )
        };

        let channel = on_progress.clone();
        channel.send(BuildProgress { percent: 0.0 }).ok();

        // How many mods there are settles the shape of the progress bar before any of them is read
        let mod_count = lock_pool(&pool)
            .map(|archives| archives.mod_count())
            .unwrap_or(0);
        let mod_weight = if mod_count == 0 { 0.0 } else { MOD_WEIGHT };
        let game_weight = 1.0 - mod_weight;

        // The packs maclarian builds its own database from, in its own order: `Shared.pak` first, then
        // `GustavX.pak`. A directory without the expansion pak builds from `Shared.pak` alone, the
        // same way the library's own build does.
        let paks: Vec<PathBuf> = ["Shared.pak", "GustavX.pak"]
            .iter()
            .map(|name| game_path.join(name))
            .filter(|path| path.is_file())
            .collect();

        let mut db = MergedDatabase::new(game_path.display().to_string());
        // A second handle for the parse callbacks, which take their channel by value: the mods below
        // keep reporting through this one
        let parse_channel = channel.clone();
        // Both packs report through the game's single progress segment, and neither file count is
        // known before it is read, so the segment is split evenly. The pieces never overlap, so the
        // bar only ever moves forward.
        //
        // Whether every pak was read is tracked for the persisted database below: a build that came
        // out short must not be written to disk, where its stamp of the sources would look unchanged
        // on the next launch and keep the incomplete index alive.
        let mut complete = true;
        let share = game_weight / paks.len().max(1) as f32;
        for (index, pak_path) in paks.iter().enumerate() {
            let base = share * index as f32;
            let progress = parse_channel.clone();
            let parsed =
                resolver.parse_pak_with_progress(pak_path, &mut db, move |current, total, _| {
                    let _ = progress.send(BuildProgress {
                        percent: base + share * parse_fraction(current, total),
                    });
                });
            if let Err(err) = parsed {
                // A pak that fails to parse costs only the resources it holds: the build carries on
                // with the others, so a broken `GustavX.pak` never costs the game's own data
                eprintln!("[maclarian] {} failed to parse: {err}", pak_path.display());
                complete = false;
            }
        }
        if db.stats().visual_count == 0 {
            eprintln!("[maclarian] no visuals parsed from the game packs");
        }

        // Materials and textures only become reachable from their visuals once the whole database
        // has been parsed. This runs before the mods are merged, so it only ever sees game data:
        // mod visuals resolve their own references as they are merged (see `mods::merge_into`).
        db.resolve_references();

        // Read out while the database is here rather than on the first detail view: maclarian does
        // not expose a material's name any other way (see `extract_materials`), and a one-off cost
        // inside a build that already runs for minutes is not something a click should pay for.
        //
        // The parameter names of the virtual texture bindings are *not* read here either: they come
        // from the materials' own templates and are read per detail view (see
        // `ensure_virtual_texture_parameters`).
        let mut materials = extract_materials(&db);

        // Mods come last, over a database that already holds everything the game provides: a mod
        // overrides what it redefines, and its visuals routinely bind the game's own materials.
        let mut mod_sources = ModSources::new();
        if mod_count > 0 {
            let mut archives = lock_pool(&pool)?;
            for index in 0..mod_count {
                let assets = mods::read_mod(&mut archives, index);
                mods::merge_into(&mut db, assets, &mut materials, &mut mod_sources);

                let done = (index + 1) as f32 / mod_count as f32;
                channel
                    .send(BuildProgress {
                        percent: game_weight + mod_weight * done,
                    })
                    .ok();
            }
        }

        let (visual_ids, visual_ids_by_id, visual_ids_by_source) =
            sorted_visual_ids(&db, &mod_sources);
        let visual_count = visual_ids.len();

        // Read off the merged maps rather than `db.stats()`: maclarian keeps the material map to
        // itself, so the mod materials are only ever in the cache above
        let stats = DatabaseStats {
            // One entry per visual GUID, so the dashboard always matches the browse list
            visual_count,
            material_count: materials.len(),
            texture_count: db.textures.len(),
            virtual_texture_count: db.virtual_textures.len(),
        };

        // Persisted while the database is still ours to borrow: the launch after this one then reads
        // the index back instead of parsing the paks again. Written before it is published (a 20 MB
        // file must not be written under the state lock), and only for a build that read everything —
        // an index missing a pak's resources would otherwise outlive the failure it came from. A
        // failed write costs the next launch a scan and nothing else, so it does not fail the build.
        if complete && visual_count > 0 {
            if let Err(err) = cache::save(&app, &game_path, &db, &materials, &mod_sources) {
                eprintln!("[cache] database not persisted: {err}");
            }
        }

        // The lock is taken again only to publish the result. A build that raced with a directory
        // switch belongs to the directory that is no longer current, so it is dropped instead.
        {
            let mut st = lock(&state)?;
            if st.game_path.as_deref() != Some(game_path.as_path()) {
                return Err(
                    "Game data directory changed while building; the result was discarded."
                        .to_string(),
                );
            }
            st.visual_ids = visual_ids;
            st.visual_ids_by_id = visual_ids_by_id;
            st.visual_ids_by_source = visual_ids_by_source;
            st.materials = materials;
            st.mod_sources = mod_sources;
            st.merged_db = Some(db);
            // Whatever the disk had to say is superseded by an index built in this session
            st.cache_status = CacheStatus::idle();
        }

        on_progress.send(BuildProgress { percent: 1.0 }).ok();

        Ok(stats)
    })
    .await
    .map_err(|err| format!("Build task terminated unexpectedly: {err}"))?
}

/// Make sure the materials of `visual_id` carry the parameter name of every virtual texture binding.
///
/// maclarian parses a binding down to its GUID, so the name it fills (`virtualtexture`,
/// `overlayvirtualtexture`, …) has to be read from the LSF documents — and the document a material
/// itself lives in cannot be derived from that material, because a merged file is always named
/// `_merged.lsf`. The material's `SourceFile` template is a real file, and it declares the same names
/// in the same order, so only the templates of this one asset's materials are read: a couple of
/// archive reads instead of a walk over every `_merged.lsf` of `Shared.pak` (see
/// `virtual_texture_params`).
///
/// The names stay in `materials` once read, so an already-named material is skipped and a second view
/// of the same asset reads nothing at all.
pub(crate) fn ensure_virtual_texture_parameters(
    state: &SharedState,
    visual_id: &str,
) -> Result<(), String> {
    let (pending, pool, game_path) = {
        let mut st = lock(state)?;

        let pending: Vec<(String, String)> = st
            .merged_db
            .as_ref()
            .and_then(|db| db.visuals_by_id.get(visual_id))
            .map(|asset| {
                asset
                    .material_ids
                    .iter()
                    .filter_map(|material_id| {
                        let material = st.materials.get(material_id)?;
                        let unresolved = material
                            .virtual_textures
                            .iter()
                            .any(|binding| binding.parameter_name.is_empty());
                        (!material.source_file.is_empty() && unresolved)
                            .then(|| (material_id.clone(), material.source_file.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Nothing to read: every binding of this asset already carries its name
        if pending.is_empty() {
            return Ok(());
        }

        let game_path = st
            .game_path
            .clone()
            .ok_or_else(|| NOT_CONFIGURED.to_string())?;
        (pending, st.pool()?, game_path)
    };

    // Read on its own: the templates come out of the archives, and the pool lock is not taken while
    // the state lock is held
    let parameters = {
        let mut archives = lock_pool(&pool)?;
        virtual_texture_params::read_parameters(&mut archives, &pending)
    };

    let mut st = lock(state)?;
    // A directory switch can land while the templates are read: the names then belong to materials
    // that are already gone
    if st.game_path.as_deref() != Some(game_path.as_path()) {
        return Ok(());
    }
    fill_virtual_texture_parameters(&mut st.materials, &parameters);
    Ok(())
}

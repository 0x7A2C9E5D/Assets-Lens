//! The commands that build the merged resource database: parsing the game's packs and merging the mods
//! over them, plus the helpers that feed it — the progress split, the visual id orders, and the
//! lazy read of a material's virtual texture parameter names.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, MergedDatabase};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::{lock, SharedState, NOT_CONFIGURED};
use crate::application::cache;
use crate::application::state::{extract_materials, AppState};
use crate::domain::app::{BuildProgress, CacheStatus, DatabaseStats};
use crate::domain::material::{fill_virtual_texture_parameters, MaterialInfo};
use crate::domain::source::ModSources;
use crate::domain::virtual_texture_params;
use crate::infrastructure::archives::{lock_pool, Archives};
use crate::infrastructure::mods;

// Share of the build progress the mods are given, when there are any. The game data is one huge archive
// and the mods are a handful of files, so the split is fixed rather than measured: it keeps the bar from
// jumping back once the game data is done
const MOD_WEIGHT: f32 = 0.2;

// The three id orders of one index: by (name, id) — the default — by id, and by (source, name, id)
type VisualOrders = (Vec<String>, Vec<String>, Vec<String>);

// What a build takes out of the state: the resolver to parse with, the directory it parses, and the pool
// the mods are read through
type BuildInputs = (Arc<GameDataResolver>, PathBuf, Arc<Mutex<Archives>>);

// The parameter names still to be read, with the pool and the directory they are read from
type PendingParameters = (Vec<(String, String)>, Arc<Mutex<Archives>>, PathBuf);

// How far the archive has been parsed, as a fraction (0 while it reports no file count)
pub(super) fn parse_fraction(current: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        current as f32 / total as f32
    }
}

// The progress bar of one build: the channel it reports through and how much of it each phase owns. The
// mod count settles the shape of the bar before any mod is read, and the split is fixed rather than
// measured. The count is read off the pool, where a failed lock counts as no mods — mods are optional
struct Report {
    channel: Channel<BuildProgress>,
    // How many mods there are; zero makes the mod phase a no-op
    mods: usize,
    // Share of the bar the game's packs own between them
    game_share: f32,
    // Share of the bar one mod owns
    mod_share: f32,
}

impl Report {
    // Open the bar of a build over `pool`, reporting through `on_progress`
    fn start(pool: &Arc<Mutex<Archives>>, on_progress: &Channel<BuildProgress>) -> Self {
        let mods = lock_pool(pool).map(|a| a.mod_count()).unwrap_or(0);
        let share = if mods == 0 { 0.0 } else { MOD_WEIGHT };
        Self {
            channel: on_progress.clone(),
            mods,
            game_share: 1.0 - share,
            mod_share: share,
        }
    }

    // Send one point of the bar, ignoring a channel that has been closed
    fn at(&self, percent: f32) {
        self.channel.send(BuildProgress { percent }).ok();
    }

    // Where one pack's parse report lands on the bar: the packs share the game's slice evenly
    fn pak_share(&self, paks: usize) -> f32 {
        self.game_share / paks.max(1) as f32
    }

    // Report one mod merged, through the mod slice of the bar
    fn mod_done(&self, index: usize) {
        let done = (index + 1) as f32 / self.mods as f32;
        self.at(self.game_share + self.mod_share * done);
    }
}

// One pack's slice of the game's share of the bar: the pieces never overlap, so the bar only ever moves
// forward
#[derive(Clone, Copy)]
struct Slice {
    share: f32,
    index: usize,
}

impl Slice {
    // Where one parse report of this pack lands on the bar
    fn percent(&self, current: usize, total: usize) -> f32 {
        self.share * self.index as f32 + self.share * parse_fraction(current, total)
    }
}

// The index a build assembles: the merged database with the caches read out of it, and the sources of
// everything the mods contribute
struct Assembly {
    db: MergedDatabase,
    materials: HashMap<String, MaterialInfo>,
    mod_sources: ModSources,
}

impl Assembly {
    // Take the parsed database and the material cache read out of it, to be merged into and given back
    fn of(db: MergedDatabase, materials: HashMap<String, MaterialInfo>) -> Self {
        Self {
            db,
            materials,
            mod_sources: ModSources::new(),
        }
    }

    // Read and merge every mod, one at a time, in the order they were enumerated
    fn merge_mods(&mut self, archives: &mut Archives, report: &Report) -> Result<(), String> {
        let Self {
            db,
            materials,
            mod_sources,
        } = self;
        for index in 0..report.mods {
            let assets = mods::read_mod(archives, index);
            mods::merge_into(db, assets, materials, mod_sources);
            report.mod_done(index);
        }
        Ok(())
    }
}

// The index a build produced, with everything publishing it needs
struct BuiltIndex {
    db: MergedDatabase,
    materials: HashMap<String, MaterialInfo>,
    mod_sources: ModSources,
    orders: VisualOrders,
    stats: DatabaseStats,
    // Whether every pak of the game data was read; an index that came out short is not persisted
    complete: bool,
}

impl BuiltIndex {
    // Assemble the index of `assembly`, with the id orders and the statistics the page reads
    fn new(assembly: Assembly, complete: bool) -> Self {
        let stats = stats_of(&assembly.db, &assembly.materials);
        let orders = sorted_visual_ids(&assembly.db, &assembly.mod_sources);
        Self {
            db: assembly.db,
            materials: assembly.materials,
            mod_sources: assembly.mod_sources,
            orders,
            stats,
            complete,
        }
    }
}

/// Build the _merged resource database; progress is pushed through a Channel
#[tauri::command]
pub async fn build_database(
    app: AppHandle,
    on_progress: Channel<BuildProgress>,
) -> Result<DatabaseStats, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || build(&app, &state, &on_progress))
        .await
        .map_err(|err| format!("Build task terminated unexpectedly: {err}"))?
}

// One build, off the main thread: what it needs is taken from the state, the game's packs and then the
// mods are parsed and merged over them, and the result is published — the state lock is held only at the
// two ends (see `build_inputs` and `publish`)
fn build(
    app: &AppHandle,
    state: &SharedState,
    on_progress: &Channel<BuildProgress>,
) -> Result<DatabaseStats, String> {
    let (resolver, game_path, pool) = build_inputs(state)?;
    on_progress.send(BuildProgress { percent: 0.0 }).ok();
    let report = Report::start(&pool, on_progress);

    let built = parse_and_merge(&resolver, &game_path, &pool, &report)?;
    persist(app, &game_path, &built);
    let stats = publish(state, &game_path, built)?;

    report.at(1.0);
    Ok(stats)
}

// Take what the build needs, releasing the lock as soon as it has them.
//
// The lock is not held through the build: parsing the game's packs runs for minutes, and every other
// command needs that lock to answer. The pool is taken here too, so it is built from the mod directory
// this build belongs to.
fn build_inputs(state: &SharedState) -> Result<BuildInputs, String> {
    let mut st = lock(state)?;
    let resolver = st
        .resolver
        .clone()
        .ok_or_else(|| NOT_CONFIGURED.to_string())?;
    let game_path = st
        .game_path
        .clone()
        .ok_or_else(|| NOT_CONFIGURED.to_string())?;
    let pool = st.pool()?;
    Ok((resolver, game_path, pool))
}

// Parse the game's packs and merge the mods over them, answering with everything publishing requires
fn parse_and_merge(
    resolver: &GameDataResolver,
    game_path: &Path,
    pool: &Arc<Mutex<Archives>>,
    report: &Report,
) -> Result<BuiltIndex, String> {
    let mut db = MergedDatabase::new(game_path.display().to_string());
    let complete = parse_game_paks(resolver, game_path, &mut db, report);

    // Materials and textures only become reachable from their visuals once the whole database has been
    // parsed, and this runs before the mods are merged so it only ever sees game data (mod visuals
    // resolve their own references as they are merged, see `mods::merge_into`)
    db.resolve_references();

    // Read out while the database is here rather than on the first detail view: maclarian does not
    // expose a material's name any other way (see `extract_materials`), and a one-off cost inside a
    // build that already runs for minutes is not something a click should pay for.
    //
    // The parameter names of the virtual texture bindings are not read here either: they come from the
    // materials' own templates and are read per detail view (see `ensure_virtual_texture_parameters`).
    let materials = extract_materials(&db);
    let mut assembly = Assembly::of(db, materials);
    merge_mods(pool, report, &mut assembly)?;
    Ok(BuiltIndex::new(assembly, complete))
}

// Parse the packs of the game's data directory, in maclarian's own order, and answer whether every one
// of them was read.
//
// A directory without the expansion pak builds from `Shared.pak` alone, the same way the library's own
// build does. A build that came out short must not be written to disk, where its stamp of the sources
// would look unchanged on the next launch and keep the incomplete index alive; the build itself carries
// on regardless, since a broken pak costs only the resources it holds.
fn parse_game_paks(
    resolver: &GameDataResolver,
    game_path: &Path,
    db: &mut MergedDatabase,
    report: &Report,
) -> bool {
    let paks = game_paks(game_path);
    let share = report.pak_share(paks.len());
    let mut complete = true;
    for (index, pak_path) in paks.iter().enumerate() {
        complete &= parse_pak(resolver, pak_path, Slice { share, index }, db, report);
    }
    warn_when_empty(db);
    complete
}

// The packs of `game_path` that exist, in the order maclarian builds its database from them:
// `Shared.pak` first, then `GustavX.pak`
fn game_paks(game_path: &Path) -> Vec<PathBuf> {
    ["Shared.pak", "GustavX.pak"]
        .iter()
        .map(|name| game_path.join(name))
        .filter(|path| path.is_file())
        .collect()
}

// Parse one pack into `db`, reporting through the slice of the bar it owns; `false` when it failed
fn parse_pak(
    resolver: &GameDataResolver,
    pak_path: &Path,
    slice: Slice,
    db: &mut MergedDatabase,
    report: &Report,
) -> bool {
    match parse_reported(resolver, pak_path, slice, db, report) {
        Ok(()) => true,
        Err(err) => failed_pak(pak_path, err),
    }
}

// Parse one pack into `db`, reporting every report through the slice of the bar it owns
fn parse_reported(
    resolver: &GameDataResolver,
    pak_path: &Path,
    slice: Slice,
    db: &mut MergedDatabase,
    report: &Report,
) -> maclarian::Result<()> {
    let progress = report.channel.clone();
    resolver.parse_pak_with_progress(pak_path, db, move |current, total, _| {
        let _ = progress.send(BuildProgress {
            percent: slice.percent(current, total),
        });
    })
}

// Report one pak that failed to parse. It costs only the resources it holds: the build carries on with
// the others, so a broken `GustavX.pak` never costs the game's own data
fn failed_pak(pak_path: &Path, err: maclarian::Error) -> bool {
    eprintln!("[maclarian] {} failed to parse: {err}", pak_path.display());
    false
}

// Report a parse that found no visuals at all: the packs above are the reason, and the index is empty
fn warn_when_empty(db: &MergedDatabase) {
    if db.stats().visual_count == 0 {
        eprintln!("[maclarian] no visuals parsed from the game packs");
    }
}

// Merge the mods over the game's own database. Mods come last, over a database that already holds
// everything the game provides: a mod overrides what it redefines, and its visuals routinely bind the
// game's own materials
fn merge_mods(
    pool: &Arc<Mutex<Archives>>,
    report: &Report,
    assembly: &mut Assembly,
) -> Result<(), String> {
    if report.mods == 0 {
        return Ok(());
    }
    let mut archives = lock_pool(pool)?;
    assembly.merge_mods(&mut archives, report)
}

// The statistics of a built index, read off the merged maps rather than `db.stats()`, because maclarian
// keeps the material map to itself and the mod materials are only ever in the cache the build filled
fn stats_of(db: &MergedDatabase, materials: &HashMap<String, MaterialInfo>) -> DatabaseStats {
    DatabaseStats {
        // One entry per visual GUID, so the dashboard always matches the browse list
        visual_count: db.visuals_by_id.len(),
        material_count: materials.len(),
        texture_count: db.textures.len(),
        virtual_texture_count: db.virtual_textures.len(),
    }
}

// Persist the index while it is still ours to borrow, so the launch after this one reads it back instead
// of parsing the packs again.
//
// Written before it is published (a 20 MB file must not be written under the state lock), and only for a
// build that read everything, since an index missing a pak's resources would otherwise outlive the
// failure it came from. A failed write costs the next launch a scan and nothing else.
fn persist(app: &AppHandle, game_path: &Path, built: &BuiltIndex) {
    if !built.complete || built.stats.visual_count == 0 {
        return;
    }
    let saved = cache::save(
        app,
        game_path,
        &built.db,
        &built.materials,
        &built.mod_sources,
    );
    if let Err(err) = saved {
        eprintln!("[cache] database not persisted: {err}");
    }
}

// Publish `built` as this session's index, unless the directory it was built from is no longer the
// current one: a build that raced with a directory switch belongs to the directory that is gone
fn publish(
    state: &SharedState,
    game_path: &Path,
    built: BuiltIndex,
) -> Result<DatabaseStats, String> {
    let mut st = lock(state)?;
    if st.game_path.as_deref() != Some(game_path) {
        return Err(discarded());
    }
    Ok(adopt_index(&mut st, built))
}

// Move the built index into the state, answering with its statistics
fn adopt_index(st: &mut AppState, built: BuiltIndex) -> DatabaseStats {
    let (visual_ids, visual_ids_by_id, visual_ids_by_source) = built.orders;
    st.visual_ids = visual_ids;
    st.visual_ids_by_id = visual_ids_by_id;
    st.visual_ids_by_source = visual_ids_by_source;
    st.materials = built.materials;
    st.mod_sources = built.mod_sources;
    st.merged_db = Some(built.db);
    // Whatever the disk had to say is superseded by an index built in this session
    st.cache_status = CacheStatus::idle();
    built.stats
}

// The error a build gets when the game data directory changed while it was running
fn discarded() -> String {
    "Game data directory changed while building; the result was discarded.".to_string()
}

// Every visual GUID in the ascending orders the list can be sorted by: by (name, id) — the default — by
// id, and by (source, name, id).
//
// Names and sources are only sort keys: neither is unique, so using them as the identity would collapse
// same-keyed visuals into one list row, and the id tiebreak keeps pagination deterministic. The source
// order puts the base game first (its records carry no source) and then the mods by name.
//
// All orders are built here, once, so a sort change only picks a different cached sequence instead of
// re-sorting every id on each page request.
pub(crate) fn sorted_visual_ids(db: &MergedDatabase, sources: &ModSources) -> VisualOrders {
    (
        by_name_order(db),
        by_id_order(db),
        by_source_order(db, sources),
    )
}

// Every visual GUID ordered by (name, id) — the default order
fn by_name_order(db: &MergedDatabase) -> Vec<String> {
    let mut rows: Vec<(&str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| (visual.name.as_str(), visual.id.as_str()))
        .collect();
    rows.sort_unstable();
    rows.into_iter().map(|(_, id)| id.to_string()).collect()
}

// Every visual GUID ordered by id
fn by_id_order(db: &MergedDatabase) -> Vec<String> {
    let mut ids: Vec<&str> = db.visuals_by_id.keys().map(String::as_str).collect();
    ids.sort_unstable();
    ids.into_iter().map(str::to_string).collect()
}

// Every visual GUID ordered by (source, name, id), the base game first: its records carry no source
fn by_source_order(db: &MergedDatabase, sources: &ModSources) -> Vec<String> {
    let mut rows: Vec<(&str, &str, &str)> = db
        .visuals_by_id
        .values()
        .map(|visual| {
            (
                source_name(sources, &visual.id),
                visual.name.as_str(),
                visual.id.as_str(),
            )
        })
        .collect();
    rows.sort_unstable();
    rows.into_iter().map(|(_, _, id)| id.to_string()).collect()
}

// The name of the mod providing `id`, or `""` for a resource that comes from the game itself
fn source_name<'a>(sources: &'a ModSources, id: &str) -> &'a str {
    sources.get(id).map(String::as_str).unwrap_or_default()
}

// Make sure the materials of `visual_id` carry the parameter name of every virtual texture binding.
//
// maclarian parses a binding down to its GUID, so the name it fills (`virtualtexture`,
// `overlayvirtualtexture`, …) has to be read from the LSF documents — and the document a material
// itself lives in cannot be derived from that material, because a merged file is always named
// `_merged.lsf`. The material's `SourceFile` template is a real file and declares the same names in the
// same order, so only the templates of this one asset's materials are read: a couple of archive reads
// instead of a walk over every `_merged.lsf` of `Shared.pak` (see `virtual_texture_params`).
//
// The names stay in `materials` once read, so an already-named material is skipped and a second view of
// the same asset reads nothing at all.
pub(crate) fn ensure_virtual_texture_parameters(
    state: &SharedState,
    visual_id: &str,
) -> Result<(), String> {
    let Some((pending, pool, game_path)) = pending_parameters(state, visual_id)? else {
        // Nothing to read: every binding of this asset already carries its name
        return Ok(());
    };
    let parameters = read_parameters(&pool, &pending)?;
    publish_parameters(state, &game_path, &parameters)
}

// The materials of `visual_id` whose parameter names still have to be read, with the pool to read them
// through and the directory they belong to; `None` when there is nothing to read at all
fn pending_parameters(
    state: &SharedState,
    visual_id: &str,
) -> Result<Option<PendingParameters>, String> {
    let mut st = lock(state)?;
    let pending = unresolved_materials(&st, visual_id);
    if pending.is_empty() {
        return Ok(None);
    }
    let game_path = st
        .game_path
        .clone()
        .ok_or_else(|| NOT_CONFIGURED.to_string())?;
    Ok(Some((pending, st.pool()?, game_path)))
}

// The (material id, source file) pairs of `visual_id` that still need their parameter names
fn unresolved_materials(st: &AppState, visual_id: &str) -> Vec<(String, String)> {
    let Some(asset) = st
        .merged_db
        .as_ref()
        .and_then(|db| db.visuals_by_id.get(visual_id))
    else {
        return Vec::new();
    };
    asset
        .material_ids
        .iter()
        .filter_map(|id| unresolved_material(st, id))
        .collect()
}

// One material that still needs its parameter names: the (material id, source file) pair to read, or
// `None` when it is already named, has no source file, or is not in the cache
fn unresolved_material(st: &AppState, material_id: &str) -> Option<(String, String)> {
    let material = st.materials.get(material_id)?;
    let unresolved = material
        .virtual_textures
        .iter()
        .any(|binding| binding.parameter_name.is_empty());
    (!material.source_file.is_empty() && unresolved)
        .then(|| (material_id.to_string(), material.source_file.clone()))
}

// Read the parameter names of `pending` out of the archives, on its own: the templates come out of the
// archives, and the pool lock is not taken while the state lock is held
fn read_parameters(
    pool: &Arc<Mutex<Archives>>,
    pending: &[(String, String)],
) -> Result<HashMap<String, Vec<String>>, String> {
    let mut archives = lock_pool(pool)?;
    Ok(virtual_texture_params::read_parameters(
        &mut archives,
        pending,
    ))
}

// Fill the names into the material cache, unless a directory switch landed while they were read: they
// would then belong to materials that are already gone
fn publish_parameters(
    state: &SharedState,
    game_path: &Path,
    parameters: &HashMap<String, Vec<String>>,
) -> Result<(), String> {
    let mut st = lock(state)?;
    if st.game_path.as_deref() != Some(game_path) {
        return Ok(());
    }
    fill_virtual_texture_parameters(&mut st.materials, parameters);
    Ok(())
}

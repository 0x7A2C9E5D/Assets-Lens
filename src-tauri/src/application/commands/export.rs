//! The command that exports a single visual asset to disk: its GR2 converted to GLB (with usable
//! textures embedded on request) alongside its textures, virtual textures and a manifest.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use maclarian::merged::{GtpMatch, VisualAsset};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::build::ensure_virtual_texture_parameters;
use super::{lock, vt_hashes, SharedState, NOT_BUILT};
use crate::application::state::AppState;
use crate::domain::export::{ExportOptions, ExportProgress, ExportResult};
use crate::domain::material::{materials_of, MaterialInfo};
use crate::domain::source::ModSources;
use crate::infrastructure::archives::Archives;
use crate::infrastructure::export::{run_export, ExportContext};

/// The material cache of one asset, the mod sources its rows are labeled with, and the pool the
/// archive reads go through
type ExportAssets = (
    HashMap<String, MaterialInfo>,
    ModSources,
    Arc<Mutex<Archives>>,
);

/// Everything one export needs, owned: the export itself runs with the state lock released.
struct ExportInputs {
    asset: VisualAsset,
    materials: HashMap<String, MaterialInfo>,
    sources: ModSources,
    vt_matches: Vec<GtpMatch>,
    pool: Arc<Mutex<Archives>>,
}

impl ExportInputs {
    /// `subject` is the asset and the page files resolved for it, `assets` the material cache and the
    /// sources the manifest names, taken together while the state was held
    fn new(subject: (VisualAsset, Vec<GtpMatch>), assets: ExportAssets) -> Self {
        let (asset, vt_matches) = subject;
        let (materials, sources, pool) = assets;
        Self {
            asset,
            materials,
            sources,
            vt_matches,
            pool,
        }
    }
}

/// Export a single visual asset: GR2 → GLB (optionally embedded textures) + textures +
/// virtual textures + asset.json.
///
/// Reading PAKs, converting and writing to disk are all slow, so this runs inside spawn_blocking;
/// the state lock is released as soon as the needed data has been fetched.
#[tauri::command]
pub async fn export_visual_asset(
    app: AppHandle,
    id: String,
    dest_dir: String,
    options: ExportOptions,
    on_progress: Channel<ExportProgress>,
) -> Result<ExportResult, String> {
    let state = app.state::<SharedState>().inner().clone();
    let dest_root = export_root(&dest_dir)?;
    let job = move || export_job(&state, &id, &dest_root, &options, &on_progress);

    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|err| format!("Export task terminated unexpectedly: {err}"))?
}

/// The directory the export writes into, checked before the task is spawned
fn export_root(dest_dir: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(dest_dir);
    if !root.exists() {
        return Err(format!("Export directory does not exist: {dest_dir}"));
    }
    Ok(root)
}

/// Take the inputs the export needs and run it over them, both off the main thread
fn export_job(
    state: &SharedState,
    id: &str,
    dest_root: &Path,
    options: &ExportOptions,
    on_progress: &Channel<ExportProgress>,
) -> Result<ExportResult, String> {
    let inputs = export_inputs(state, id, options)?;
    export_with(&inputs, dest_root, options, on_progress)
}

/// The inputs of one export, taken while the state lock is held.
///
/// The manifest states the parameter of every virtual texture binding, both on the material rows and
/// on the virtual texture rows. Those names are not in the parsed database, so they are read off this
/// asset's material templates first — and only while some binding of this asset has no name yet (see
/// `ensure_virtual_texture_parameters`).
fn export_inputs(
    state: &SharedState,
    id: &str,
    options: &ExportOptions,
) -> Result<ExportInputs, String> {
    ensure_virtual_texture_parameters(state, id)?;
    take_export_inputs(state, id, options)
}

/// The inputs read under the state lock, released again before anything is written
fn take_export_inputs(
    state: &SharedState,
    id: &str,
    options: &ExportOptions,
) -> Result<ExportInputs, String> {
    let mut st = lock(state)?;
    let subject = export_subject(&mut st, id, options)?;
    let assets = export_assets(&mut st, &subject.0)?;
    Ok(ExportInputs::new(subject, assets))
}

/// The asset of `id` and the page files resolved for it
fn export_subject(
    st: &mut AppState,
    id: &str,
    options: &ExportOptions,
) -> Result<(VisualAsset, Vec<GtpMatch>), String> {
    let asset = asset_of(st, id)?;
    let vt_matches = page_files(st, &asset, options);
    Ok((asset, vt_matches))
}

/// The asset `id` names, cloned out of the index
fn asset_of(st: &AppState, id: &str) -> Result<VisualAsset, String> {
    st.merged_db
        .as_ref()
        .and_then(|db| db.visuals_by_id.get(id))
        .cloned()
        .ok_or_else(|| NOT_BUILT.to_string())
}

/// The page files resolved for `asset`, looked up only when virtual textures are part of the export:
/// with an empty list the manifest rows simply carry no path and no archive
fn page_files(st: &mut AppState, asset: &VisualAsset, options: &ExportOptions) -> Vec<GtpMatch> {
    if !options.texture_format.is_export() {
        return Vec::new();
    }
    st.vt_matches(&vt_hashes(asset))
}

/// The material cache entries of `asset`, the mod sources the manifest labels every row with, and the
/// archive pool the reads share
fn export_assets(st: &mut AppState, asset: &VisualAsset) -> Result<ExportAssets, String> {
    // Cut out of the cache while the state is held: the manifest names the materials of the asset,
    // and the cache they live in is not handed to the export
    let materials = materials_of(&asset.material_ids, &st.materials);
    // Taken whole: unlike the material cache it only holds the resources the mods provide, and the
    // manifest labels every row of the asset with it
    let sources = st.mod_sources.clone();
    // Shared with the previews: the archives this export needs are usually open already
    Ok((materials, sources, st.pool()?))
}

/// Run the export over the taken inputs, reporting progress through the channel
fn export_with(
    inputs: &ExportInputs,
    dest_root: &Path,
    options: &ExportOptions,
    on_progress: &Channel<ExportProgress>,
) -> Result<ExportResult, String> {
    run_export(&export_context(inputs), dest_root, options, &|progress| {
        let _ = on_progress.send(progress);
    })
}

/// The borrow `run_export` takes of one export's inputs
fn export_context(inputs: &ExportInputs) -> ExportContext<'_> {
    ExportContext {
        asset: &inputs.asset,
        materials: &inputs.materials,
        sources: &inputs.sources,
        vt_matches: &inputs.vt_matches,
        pool: &inputs.pool,
    }
}

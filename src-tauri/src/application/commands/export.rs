//! The command that exports a single visual asset to disk: its GR2 converted to GLB (with usable
//! textures embedded on request) alongside its textures, virtual textures and a manifest.

use std::path::PathBuf;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::build::ensure_virtual_texture_parameters;
use super::{lock, vt_hashes, SharedState, NOT_BUILT};
use crate::domain::export::{ExportOptions, ExportProgress, ExportResult};
use crate::domain::material::materials_of;
use crate::infrastructure::export::{run_export, ExportContext};

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
    let dest_root = PathBuf::from(&dest_dir);

    if !dest_root.exists() {
        return Err(format!("Export directory does not exist: {dest_dir}"));
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<ExportResult, String> {
        // The manifest states the parameter of every virtual texture binding, both on the material
        // rows and on the virtual texture rows. Those names are not in the parsed database, so they
        // are read off this asset's material templates first — and only while some binding of this
        // asset has no name yet (see `ensure_virtual_texture_parameters`).
        ensure_virtual_texture_parameters(&state, &id)?;

        let (pool, asset, materials, sources, vt_matches) = {
            let mut st = lock(&state)?;
            let asset = st
                .merged_db
                .as_ref()
                .and_then(|db| db.visuals_by_id.get(&id))
                .cloned()
                .ok_or_else(|| NOT_BUILT.to_string())?;
            // Page files are looked up only when virtual textures are part of the export; with an
            // empty list the manifest rows simply carry no path and no archive
            let vt_matches = if options.texture_format.is_export() {
                st.vt_matches(&vt_hashes(&asset))
            } else {
                Vec::new()
            };
            // Cut out of the cache while the state is held: the manifest names the materials of the
            // asset, and the cache they live in is not handed to the export
            let materials = materials_of(&asset.material_ids, &st.materials);
            // Taken whole: unlike the material cache it only holds the resources the mods provide,
            // and the manifest labels every row of the asset with it
            let sources = st.mod_sources.clone();
            // Shared with the previews: the archives this export needs are usually open already
            (st.pool()?, asset, materials, sources, vt_matches)
        };

        run_export(
            &ExportContext {
                asset: &asset,
                materials: &materials,
                sources: &sources,
                vt_matches: &vt_matches,
                pool: &pool,
            },
            &dest_root,
            &options,
            &|progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
    .await
    .map_err(|err| format!("Export task terminated unexpectedly: {err}"))?
}

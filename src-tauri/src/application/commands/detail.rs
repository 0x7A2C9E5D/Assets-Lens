//! The commands that read a single visual asset: its full detail (with the page file sizes of its
//! virtual textures) and the GR2 mesh converted to GLB for the three.js preview.

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use tauri::{AppHandle, Manager};

use super::build::ensure_virtual_texture_parameters;
use super::{lock, vt_hashes, SharedState};
use crate::domain::virtual_textures::{self, match_for_hash};
use crate::domain::visual::{ModelPreview, VisualAssetDetail};
use crate::infrastructure::archives::{lock_pool, Pak};

/// Query the detail of a single visual asset by its GUID (names are not unique).
///
/// Reading a page file size is the only archive work here, and it is off the main thread: the GTS
/// that carries it is one file read, not a scan.
#[tauri::command]
pub async fn get_visual(app: AppHandle, id: String) -> Result<Option<VisualAssetDetail>, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<Option<VisualAssetDetail>, String> {
        // The binding chips show the parameter each virtual texture fills. Those names are not in the
        // parsed database, so they are read off this asset's material templates first — and only
        // while some binding of this asset has no name yet (see
        // `ensure_virtual_texture_parameters`).
        ensure_virtual_texture_parameters(&state, &id)?;

        let (mut detail, matches, pool, mut page_file_sizes) = {
            let mut st = lock(&state)?;
            // Owned rather than borrowed: the GTex lookup below needs `&mut` state (it hands the
            // database to a resolver and takes it back), which no borrow of the database outlives.
            let asset = match st
                .merged_db
                .as_ref()
                .and_then(|db| db.visuals_by_id.get(&id))
            {
                Some(asset) => asset.clone(),
                None => return Ok(None),
            };

            let hashes = vt_hashes(&asset);
            let matches = if hashes.is_empty() {
                Vec::new()
            } else {
                st.vt_matches(&hashes)
            };
            let detail = VisualAssetDetail::new(&asset, &matches, &st.materials, &st.mod_sources);

            // The size cache travels with the detail and is put back at the end: it is only ever
            // touched here, and holding the state lock while the archives are read is what the
            // block below exists to avoid. `pool()` takes `&mut` as well, so it runs first
            let page_file_sizes = std::mem::take(&mut st.page_file_sizes);
            (detail, matches, st.pool().ok(), page_file_sizes)
        };

        // Page file sizes need the archives: a GTS is one file read from the virtual texture archive
        // (no scan), and each of them serves every page file of its tile set, so `page_file_size`
        // sees to reading one only once
        if let Some(pool) = pool {
            match lock_pool(&pool) {
                Ok(mut archives) => {
                    for vt in &mut detail.virtual_textures {
                        let size = match_for_hash(&matches, &vt.hash).and_then(|matched| {
                            virtual_textures::page_file_size(
                                &mut archives,
                                &mut page_file_sizes,
                                matched,
                                &vt.hash,
                            )
                        });
                        vt.set_size(size);
                    }
                }
                // A size is decoration: an unavailable pool leaves the rows without one
                Err(err) => eprintln!("[maclarian] page file sizes unavailable: {err}"),
            }
        }

        // Back into the state, for the next detail view of this game directory
        lock(&state)?.page_file_sizes = page_file_sizes;

        Ok(Some(detail))
    })
    .await
    .map_err(|err| format!("Detail task terminated unexpectedly: {err}"))?
}

/// Read the GR2 mesh of a visual asset and convert it to GLB for the frontend three.js preview
/// (geometry only, no textures). Everything stays in memory: Shared.pak bytes → GLB → Base64, with
/// no intermediate files.
/// The conversion (BitKnit decompression + parsing) can take several seconds, so it runs inside
/// spawn_blocking to avoid freezing the UI
#[tauri::command]
pub async fn get_visual_preview(app: AppHandle, path: String) -> Result<ModelPreview, String> {
    let state = app.state::<SharedState>().inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<ModelPreview, String> {
        // The pool is shared with every other command, so a preview no longer re-opens the archives.
        // Only the handle is taken here: the state lock is released before anything is read.
        let pool = {
            let mut st = lock(&state)?;
            st.pool()?
        };

        // maclarian's own reader compares PAK entries with an exact `==` on the raw path, which
        // never matches on Windows (`\` vs `/`); the `Archives` pool normalizes separators and casing
        // instead.
        let gr2_bytes = lock_pool(&pool)?
            .read_from(Pak::Models, &path)
            .map_err(|err| {
                eprintln!("[maclarian] find gr2 {path} failed: {err}");
                err
            })?;

        let glb = convert_gr2_bytes_to_glb(&gr2_bytes).map_err(|err| {
            eprintln!("[maclarian] gr2 -> glb failed for {path}: {err}");
            // Pass through unchanged: for a mesh-less GR2 maclarian returns
            // "No meshes found in GR2 file (...)", which the frontend maps to dedicated copy;
            // anything else falls through to the generic failure message
            err.to_string()
        })?;

        Ok(ModelPreview {
            base64: base64::engine::general_purpose::STANDARD.encode(&glb),
        })
    })
    .await
    .map_err(|err| format!("Preview task terminated unexpectedly: {err}"))?
}

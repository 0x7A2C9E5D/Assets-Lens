//! The commands that read a single visual asset: its full detail (with the page file sizes of its
//! virtual textures) and the GR2 mesh converted to GLB for the three.js preview.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine;
use maclarian::converter::gr2_gltf::convert_gr2_bytes_to_glb;
use maclarian::merged::{GtpMatch, VisualAsset};
use tauri::{AppHandle, Manager};

use super::build::ensure_virtual_texture_parameters;
use super::{lock, vt_hashes, SharedState};
use crate::application::state::AppState;
use crate::domain::virtual_textures::{self, match_for_hash, PageFileSizes};
use crate::domain::visual::{ModelPreview, VisualAssetDetail};
use crate::infrastructure::archives::{lock_pool, Archives, Pak};

/// What one detail view reads out of the state in a single pass
type DetailParts = (
    VisualAssetDetail,
    Vec<GtpMatch>,
    Option<Arc<Mutex<Archives>>>,
    HashMap<String, PageFileSizes>,
);

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
        detail_with_sizes(&state, &id)
    })
    .await
    .map_err(|err| format!("Detail task terminated unexpectedly: {err}"))?
}

/// The detail of `id` with the page file size of every virtual texture it binds, or `None` when the
/// index holds no such visual.
///
/// The sizes are read with the state lock released, then the size cache is put back for the next
/// detail view of this game directory.
fn detail_with_sizes(state: &SharedState, id: &str) -> Result<Option<VisualAssetDetail>, String> {
    let Some((mut detail, matches, pool, mut page_file_sizes)) = take_detail(state, id)? else {
        return Ok(None);
    };
    if let Some(pool) = pool {
        fill_page_sizes(&pool, &mut detail, &matches, &mut page_file_sizes);
    }
    lock(state)?.page_file_sizes = page_file_sizes;
    Ok(Some(detail))
}

/// Everything one detail view reads out of the state, in one pass under one lock.
///
/// Owned rather than borrowed: the GTex lookup below needs `&mut` state (it hands the database to a
/// resolver and takes it back), which no borrow of the database outlives. `pool()` takes `&mut` as
/// well, so it runs last.
fn take_detail(state: &SharedState, id: &str) -> Result<Option<DetailParts>, String> {
    let mut st = lock(state)?;
    let asset = st
        .merged_db
        .as_ref()
        .and_then(|db| db.visuals_by_id.get(id));
    let Some(asset) = asset.cloned() else {
        return Ok(None);
    };

    let matches = resolved_matches(&mut st, &asset);
    let detail = VisualAssetDetail::new(&asset, &matches, &st.materials, &st.mod_sources);
    // The size cache travels with the detail and is put back at the end: it is only ever touched
    // here, and holding the state lock while the archives are read is what the caller avoids
    let page_file_sizes = std::mem::take(&mut st.page_file_sizes);
    let pool = st.pool().ok();
    Ok(Some((detail, matches, pool, page_file_sizes)))
}

/// The page files resolved for `asset`'s virtual textures. An asset that binds none is not looked up
/// at all, so maclarian is not asked for an empty answer.
fn resolved_matches(st: &mut AppState, asset: &VisualAsset) -> Vec<GtpMatch> {
    let hashes = vt_hashes(asset);
    if hashes.is_empty() {
        return Vec::new();
    }
    st.vt_matches(&hashes)
}

/// Fill in the page file size of every virtual texture of `detail`.
///
/// A size is decoration: a pool that cannot be locked leaves the rows without one rather than failing
/// the detail view.
fn fill_page_sizes(
    pool: &Arc<Mutex<Archives>>,
    detail: &mut VisualAssetDetail,
    matches: &[GtpMatch],
    page_file_sizes: &mut HashMap<String, PageFileSizes>,
) {
    let Some(mut archives) = locked_archives(pool) else {
        return;
    };
    for vt in &mut detail.virtual_textures {
        vt.set_size(page_size(&mut archives, page_file_sizes, matches, &vt.hash));
    }
}

/// The archives pool, locked for one read, or `None` when it cannot be locked — which is logged here
/// because the answer is the same as a failed lookup: a row without a size
fn locked_archives(pool: &Arc<Mutex<Archives>>) -> Option<MutexGuard<'_, Archives>> {
    match lock_pool(pool) {
        Ok(archives) => Some(archives),
        Err(err) => {
            eprintln!("[maclarian] page file sizes unavailable: {err}");
            None
        }
    }
}

/// The size of the page file `hash` resolves to, or `None` when no page file matches it
fn page_size(
    archives: &mut Archives,
    page_file_sizes: &mut HashMap<String, PageFileSizes>,
    matches: &[GtpMatch],
    hash: &str,
) -> Option<(u32, u32)> {
    match_for_hash(matches, hash).and_then(|matched| {
        virtual_textures::page_file_size(archives, page_file_sizes, matched, hash)
    })
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
        let pool = shared_pool(&state)?;
        let glb = glb_of(&mesh_bytes(&pool, &path)?, &path)?;
        Ok(ModelPreview {
            base64: base64::engine::general_purpose::STANDARD.encode(&glb),
        })
    })
    .await
    .map_err(|err| format!("Preview task terminated unexpectedly: {err}"))?
}

/// The PAK pool this session shares, taken with the state lock released before anything is read
fn shared_pool(state: &SharedState) -> Result<Arc<Mutex<Archives>>, String> {
    let mut st = lock(state)?;
    st.pool()
}

/// The raw GR2 bytes of `path` out of the model archive.
///
/// maclarian's own reader compares PAK entries with an exact `==` on the raw path, which never matches
/// on Windows (`\` vs `/`); the `Archives` pool normalizes separators and casing instead.
fn mesh_bytes(pool: &Arc<Mutex<Archives>>, path: &str) -> Result<Vec<u8>, String> {
    lock_pool(pool)?
        .read_from(Pak::Models, path)
        .map_err(|err| {
            eprintln!("[maclarian] find gr2 {path} failed: {err}");
            err
        })
}

/// `bytes` as GLB. A mesh-less GR2 fails here, with maclarian's own message passed through unchanged:
/// the frontend maps "No meshes found in GR2 file (...)" to dedicated copy, and anything else falls
/// through to the generic failure message.
fn glb_of(bytes: &[u8], path: &str) -> Result<Vec<u8>, String> {
    convert_gr2_bytes_to_glb(bytes).map_err(|err| {
        eprintln!("[maclarian] gr2 -> glb failed for {path}: {err}");
        err.to_string()
    })
}

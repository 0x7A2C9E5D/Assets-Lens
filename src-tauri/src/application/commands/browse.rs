//! The commands that report on the index and list what is in it: the statistics and the cache status
//! the dashboard reads, the app's own metadata, and the paged browse list.

use tauri::State;

use super::{current_stats, lock, SharedState, NOT_BUILT};
use crate::domain::app::{AppInfo, CacheStatus, DatabaseStats, Page};
use crate::domain::visual::VisualSummary;

/// Current database statistics (None when not built yet)
#[tauri::command]
pub fn db_stats(state: State<'_, SharedState>) -> Result<Option<DatabaseStats>, String> {
    let st = lock(&state)?;
    Ok(current_stats(&st))
}

/// Where the index the statistics above describe came from: read back from disk, or built here.
///
/// Read after the game directory has been restored rather than before: until a directory is selected
/// there is nothing to report, and selecting one is exactly when the persisted file is looked for.
#[tauri::command]
pub fn cache_status(state: State<'_, SharedState>) -> Result<CacheStatus, String> {
    Ok(lock(&state)?.cache_status.clone())
}

/// App metadata for the About page (compile-time values, no state required)
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Browse visual assets page by page. `keyword` is an optional filter matched against the asset
/// name or its GUID (so a pasted ID finds its row); `sort` picks the column and `descending` the
/// direction, both resolved against the orders cached when the database was built. `origin` splits
/// the index in two: "mod" keeps only what a mod provides, "base" only the game's own resources,
/// and anything else (including a missing value) keeps both.
#[tauri::command]
pub fn list_visuals(
    state: State<'_, SharedState>,
    offset: usize,
    limit: usize,
    keyword: Option<String>,
    sort: Option<String>,
    descending: Option<bool>,
    origin: Option<String>,
) -> Result<Page<VisualSummary>, String> {
    let st = lock(&state)?;
    let db = st.merged_db.as_ref().ok_or_else(|| NOT_BUILT.to_string())?;

    // "id" opts into the GUID order and "source" into the mod-name order; anything else (including a
    // missing value) keeps the name order
    let ids = match sort.as_deref() {
        Some("id") => &st.visual_ids_by_id,
        Some("source") => &st.visual_ids_by_source,
        _ => &st.visual_ids,
    };

    let keyword = keyword
        .map(|kw| kw.trim().to_lowercase())
        .filter(|kw| !kw.is_empty());

    let mut matched: Vec<&String> = ids
        .iter()
        .filter(|id| {
            // Mod-provided ids are exactly the ones the source table lists, so membership in it is
            // the whole test; the game's own resources are never written there.
            let from_mod = st.mod_sources.contains_key(id.as_str());
            match origin.as_deref() {
                Some("mod") if !from_mod => return false,
                Some("base") if from_mod => return false,
                _ => {}
            }

            keyword.as_deref().is_none_or(|kw| {
                // The cached order only holds GUIDs, so the name to match against comes from the
                // DB. GUIDs are ASCII, so folding them stays a cheap ASCII-lowercase compare.
                id.as_str().to_ascii_lowercase().contains(kw)
                    || db
                        .visuals_by_id
                        .get(*id)
                        .is_some_and(|visual| visual.name.to_lowercase().contains(kw))
            })
        })
        .collect();

    // The cached sequences are ascending and a page is sliced out of the matches, so a descending
    // request flips the whole match set rather than the page
    if descending.unwrap_or(false) {
        matched.reverse();
    }

    let total = matched.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    let items: Vec<VisualSummary> = matched[start..end]
        .iter()
        .filter_map(|id| db.visuals_by_id.get(*id))
        .map(|visual| VisualSummary::of(visual, &st.mod_sources))
        .collect();

    Ok(Page {
        items,
        total,
        offset: start,
    })
}

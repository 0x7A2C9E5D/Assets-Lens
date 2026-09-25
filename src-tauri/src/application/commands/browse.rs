//! The commands that report on the index and list what is in it: the statistics and the cache status
//! the dashboard reads, the app's own metadata, and the paged browse list.

use maclarian::merged::MergedDatabase;
use tauri::State;

use super::{current_stats, lock, SharedState, NOT_BUILT};
use crate::application::state::AppState;
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

/// The filter side of a browse query, resolved before the list is walked: the origin split, and the
/// direction the page is read in.
struct Filter<'a> {
    origin: Option<&'a str>,
    descending: bool,
}

impl<'a> Filter<'a> {
    /// The filter as the query states it, with the direction a missing value gets
    fn of(origin: Option<&'a str>, descending: Option<bool>) -> Self {
        Self {
            origin,
            descending: descending.unwrap_or(false),
        }
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
    let ids = ordered_ids(&st, sort.as_deref());
    let filter = Filter::of(origin.as_deref(), descending);
    Ok(browse_page(&st, db, ids, keyword, &filter, offset, limit))
}

/// The cached order `sort` picks: the GUID order, the mod-name order, or — for anything else,
/// including a missing value — the name order
fn ordered_ids<'a>(st: &'a AppState, sort: Option<&str>) -> &'a [String] {
    match sort {
        Some("id") => &st.visual_ids_by_id,
        Some("source") => &st.visual_ids_by_source,
        _ => &st.visual_ids,
    }
}

/// One page of the browse list: `ids` in the order `sort` picked, filtered by the keyword — trimmed
/// and folded for comparison — and by `filter`, and sliced from `offset` for `limit` rows
fn browse_page(
    st: &AppState,
    db: &MergedDatabase,
    ids: &[String],
    keyword: Option<String>,
    filter: &Filter<'_>,
    offset: usize,
    limit: usize,
) -> Page<VisualSummary> {
    let keyword = keyword
        .map(|kw| kw.trim().to_lowercase())
        .filter(|kw| !kw.is_empty());
    let matched = matched_ids(st, db, ids, keyword.as_deref(), filter);
    page_of(db, st, &matched, offset, limit)
}

/// The ids that pass both filters, in the direction `filter` asks for.
///
/// The cached sequences are ascending and a page is sliced out of the matches, so a descending request
/// flips the whole match set rather than the page.
fn matched_ids<'a>(
    st: &AppState,
    db: &MergedDatabase,
    ids: &'a [String],
    keyword: Option<&str>,
    filter: &Filter<'_>,
) -> Vec<&'a String> {
    let matched: Vec<&String> = ids
        .iter()
        .filter(|id| keeps(st, db, id, keyword, filter.origin))
        .collect();
    in_direction(matched, filter.descending)
}

/// `matched` flipped when the page is read in descending order
fn in_direction(mut matched: Vec<&String>, descending: bool) -> Vec<&String> {
    if descending {
        matched.reverse();
    }
    matched
}

/// Whether one id of the sorted list is in the page: both filters pass
fn keeps(
    st: &AppState,
    db: &MergedDatabase,
    id: &str,
    keyword: Option<&str>,
    origin: Option<&str>,
) -> bool {
    let keyword_ok = keyword.is_none_or(|kw| matches_keyword(db, id, kw));
    keyword_ok && passes_origin(st, id, origin)
}

/// Whether an id passes the `origin` split: "mod" keeps only what a mod provides, "base" only the
/// game's own resources, and anything else (including a missing value) keeps both.
///
/// Mod-provided ids are exactly the ones the source table lists, so membership in it is the whole
/// test; the game's own resources are never written there.
fn passes_origin(st: &AppState, id: &str, origin: Option<&str>) -> bool {
    let from_mod = st.mod_sources.contains_key(id);
    match origin {
        Some("mod") => from_mod,
        Some("base") => !from_mod,
        _ => true,
    }
}

/// Whether `id` carries `keyword`: in its GUID — folded the cheap ASCII way, since a GUID is ASCII —
/// or in the name of the visual it names, which the cached order does not hold
fn matches_keyword(db: &MergedDatabase, id: &str, keyword: &str) -> bool {
    id.to_ascii_lowercase().contains(keyword)
        || db
            .visuals_by_id
            .get(id)
            .is_some_and(|visual| visual.name.to_lowercase().contains(keyword))
}

/// The window of matches the page shows, as the list rows the frontend renders
fn page_of(
    db: &MergedDatabase,
    st: &AppState,
    matched: &[&String],
    offset: usize,
    limit: usize,
) -> Page<VisualSummary> {
    let total = matched.len();
    let start = offset.min(total);
    let end = (start + limit).min(total);
    Page {
        items: page_items(db, st, &matched[start..end]),
        total,
        offset: start,
    }
}

/// The rows of one page: the matched GUIDs of its window, as the list items the frontend renders
fn page_items(db: &MergedDatabase, st: &AppState, window: &[&String]) -> Vec<VisualSummary> {
    window
        .iter()
        .filter_map(|id| db.visuals_by_id.get(*id))
        .map(|visual| VisualSummary::of(visual, &st.mod_sources))
        .collect()
}

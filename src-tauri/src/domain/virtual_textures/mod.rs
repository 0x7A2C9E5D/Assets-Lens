//! Virtual textures: the row an asset's list carries, and the page files behind it.
//!
//! A GTS is the metadata of a whole tile set (`<Base>_<index>.gts`), not the companion of one page
//! file, so the extractor resolves a page file by looking its hash up in the GTS metadata — trying
//! several candidates is safe, because a GTS from another tile set fails instead of exporting the
//! wrong pixels.
//!
//! Both live inside the archives as raw blocks while `VirtualTextureExtractor` takes file paths, so
//! they are staged to a directory first. Finding them is archive work: this module only produces
//! paths, and `infrastructure::export` turns them into artifacts.

use std::collections::HashMap;

use maclarian::merged::{GtpMatch, VirtualTextureRef, VisualAsset};
use serde::Serialize;

use crate::domain::material::{virtual_texture_parameter, MaterialInfo};
use crate::domain::source::{source_of, ModSources};

mod page_files;
mod staging;

pub use page_files::{page_file_size, PageFileSizes};
pub use staging::stage_sources;

/// Streaming virtual texture reference (GTex): a row of the asset's virtual texture list, and —
/// copied — the entry a material binding of the export manifest carries, which is why it is `Clone`.
/// Its `width` / `height` are filled by whoever holds the archives (`export::fill_vt_sizes`).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
    /// Mod providing this virtual texture; absent for the game's own
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub hash: String,
    /// Page file (`.gtp`) inside its archive; empty when no lookup was run or nothing matched
    pub path: String,
    /// Pixel size of this page file, read out of its tile set's GTS through
    /// `virtual_textures::page_file_size`; it is the same box the extractor writes as its DDS.
    /// `None` when the hash resolved to no page file, that GTS could not be parsed, or the caller
    /// does not read the GTS at all (the row then renders without a size)
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Parameter the binding fills (e.g. `virtualtexture`), read off the asset's materials — it
    /// belongs to the binding, not to the resource. One name per row, so an asset binding the same
    /// virtual texture through several materials shows only the first here; each material states its
    /// own name in its `bindings`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
}

impl VirtualTextureSummary {
    /// `matched` is the page file resolved for this hash — `None` when the lookup found nothing
    /// (in which case the row renders without path and archive) or was skipped by the caller
    pub fn new(value: &VirtualTextureRef, matched: Option<&GtpMatch>) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            // Settled by the caller, which is the only place holding the source map
            source: None,
            hash: value.gtex_hash.clone(),
            path: matched.map(|m| m.gtp_path.clone()).unwrap_or_default(),
            // Settled later: reading the size needs the archives, which this constructor is not given
            width: None,
            height: None,
            parameter_name: None,
        }
    }

    /// Fill in the page file size. A failed lookup leaves both fields unset: the size is
    /// decoration like the archive name, and must not take a detail view or an export down with it.
    pub fn set_size(&mut self, size: Option<(u32, u32)>) {
        if let Some((width, height)) = size {
            self.width = Some(width);
            self.height = Some(height);
        }
    }
}

/// The page file resolved for one GTex hash. `GtpMatch::gtex_hash` echoes the hash that was
/// searched for, so this is a plain lookup rather than a hash comparison.
pub fn match_for_hash<'a>(matches: &'a [GtpMatch], hash: &str) -> Option<&'a GtpMatch> {
    let hash = hash.trim();
    matches
        .iter()
        .find(|matched| matched.gtex_hash.eq_ignore_ascii_case(hash))
}

/// The virtual textures of `value`, in the asset's own order: the virtual texture list of the detail
/// panel, and the source the export manifest builds its material rows from. The parameter name stays
/// empty until the caller resolves it, since the templates carrying it are not read here.
pub fn virtual_texture_summaries(
    value: &VisualAsset,
    matches: &[GtpMatch],
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
) -> Vec<VirtualTextureSummary> {
    value
        .virtual_textures
        .iter()
        .map(|vt| virtual_texture_summary(vt, value, matches, materials, sources))
        .collect()
}

// One row of the virtual texture list: the resource with its page file, labeled with the mod that
// provides it and the parameter the asset's materials bind it with
fn virtual_texture_summary(
    vt: &VirtualTextureRef,
    value: &VisualAsset,
    matches: &[GtpMatch],
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
) -> VirtualTextureSummary {
    let mut summary = VirtualTextureSummary::new(vt, match_for_hash(matches, &vt.gtex_hash));
    summary.source = source_of(sources, &vt.id);
    summary.parameter_name = virtual_texture_parameter(&vt.id, &value.material_ids, materials);
    summary
}

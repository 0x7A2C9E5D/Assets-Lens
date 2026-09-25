//! Virtual textures: the row an asset's list carries, and the page files behind it.
//!
//! Page files: stage the page file a `GtpMatch` names plus the GTS that covers it.
//!
//! A GTS is not the companion of one-page file: it is the metadata of a whole tile set
//! (`<Base>_<index>.gts`), listing every page file of that set, so one GTS serves many GTPs. The
//! extractor resolves a page file by looking its hash up in the GTS metadata, which is what makes
//! trying several candidates safe — a GTS from another tile set fails instead of exporting the
//! wrong pixels.
//!
//! Both live inside the archives as raw blocks, while `VirtualTextureExtractor` takes file paths, so
//! they are written to a staging directory first. Finding them is archive work rather than export
//! work: this module only produces paths, and `infrastructure::export` turns them into exported
//! artifacts.

use std::collections::HashMap;

use maclarian::merged::{GtpMatch, VirtualTextureRef, VisualAsset};
use serde::Serialize;

use crate::domain::material::{virtual_texture_parameter, MaterialInfo};
use crate::domain::source::{source_of, ModSources};

mod page_files;
mod staging;

pub use page_files::{page_file_size, PageFileSizes};
pub use staging::stage_sources;

/// Streaming virtual texture reference (GTex).
///
/// A row of the asset's virtual texture list (the detail panel), and — copied — the entry the export
/// manifest material binding it carries, which is why it is `Clone`. Its `width` / `height` are filled
/// by whoever holds the archives, for the detail rows and for the entries inside a material alike (see
/// `export::fill_vt_sizes`).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
    /// Mod providing this virtual texture; absent when it comes from the game (see
    /// `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub hash: String,
    /// Page file (`.gtp`) inside its archive; empty when no lookup was run or nothing matched
    pub path: String,
    /// Pixel size of this page file, read out of its tile set's GTS — by `get_visual` for a detail
    /// row and by `export::fill_vt_sizes` for the export manifest, both off the same
    /// `virtual_textures::page_file_size`. It is the same box the extractor writes as its DDS.
    /// `None` when the hash resolved to no page file, that GTS could not be parsed, or the caller
    /// does not read the GTS at all (the row then renders without a size)
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Parameter the binding fills (e.g. `virtualtexture`), read off the asset's materials — it
    /// belongs to the binding, not to the resource. Absent from the JSON while unset: the names come
    /// from the materials' templates (see `domain::material::fill_virtual_texture_parameters`), which a detail view
    /// and an export both read up front (`application::commands::ensure_virtual_texture_parameters`).
    ///
    /// One name per row, so an asset that binds a virtual texture through several materials with
    /// different parameters shows only the first here; each material states its own name in its
    /// `bindings` (see `material::fill_material_bindings`).
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

/// The virtual textures of `value`, in the asset's own order, each carrying the parameter the binding
/// fills.
///
/// The virtual texture list of the detail panel, and the source the export manifest builds its material
/// rows from (see `material::manifest_materials`). The parameter name is only ever filled once the caller
/// resolved it — the templates carrying it are read by a detail view and by an export
/// (`application::commands::ensure_virtual_texture_parameters`), not by this function — so a row
/// stays without one until then. Which material binds a virtual texture is likewise stated on the
/// material rows (see `material::fill_material_bindings`).
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

/// One row of the virtual texture list: the resource with its page file, labeled with the mod that
/// provides it and the parameter the asset's materials bind it with
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

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
use std::fs;
use std::path::{Path, PathBuf};

use maclarian::merged::{GtpMatch, VirtualTextureRef, VisualAsset};
use serde::Serialize;

use crate::domain::material::{virtual_texture_parameter, MaterialInfo};
use crate::domain::naming::derive_gts_path;
use crate::domain::source::{source_of, ModSources};
use crate::infrastructure::archives::{Archives, Pak};

/// Files staged for one-page file, both already on disk
pub struct StagedSources {
    /// Page file (`.gtp`), named after its archive entry
    pub gtp: PathBuf,
    /// Candidate GTS files, ordered by likelihood. Normally the first one hits: a page file belongs
    /// to exactly one tile set, and every page file of that set shares its GTS.
    pub gts_candidates: Vec<PathBuf>,
}

/// Stage the page file `matched` names plus the GTS that covers it into `shared`.
///
/// Staged files are reused by file name: one GTS carries the metadata of a whole tile set and is
/// shared by all of its page files, so re-reading it from the archives for every virtual texture
/// would be wasted work.
pub fn stage_sources(
    pak: &mut Archives,
    matched: &GtpMatch,
    shared: &Path,
) -> Result<StagedSources, String> {
    let gtp_rel = matched.gtp_path.as_str();
    let gtp_name = Path::new(gtp_rel)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid GTP path: {gtp_rel}"))?;

    let gtp = shared.join(gtp_name);
    if !gtp.exists() {
        // Page files are read from the virtual texture archive and nowhere else
        let bytes = pak.read_from(Pak::VirtualTextures, gtp_rel)?;
        fs::write(&gtp, bytes).map_err(|e| format!("Failed to stage GTP: {e}"))?;
    }

    let gts_candidates = stage_gts_candidates(pak, matched, shared)?;
    Ok(StagedSources {
        gtp,
        gts_candidates,
    })
}

/// Pixel size of every page file of one tile set, paired with the page file name it belongs to.
///
/// Layer 0 only — the albedo page the game shows and the first one the extractor writes; `None`
/// for a page file that stores no layer 0 tiles.
pub type PageFileSizes = Vec<(String, Option<(u32, u32)>)>;

/// Pixel size of the page file `hash` names, in the resolution that page file really stores.
///
/// A GTS header carries the tile geometry (`tile_width` / `tile_height` / `tile_border`) and, per
/// mip level, that level's tile grid — but level 0's grid describes the whole *tile set*, not one-page file:
/// `Albedo_Normal_Physical_2` claims 512 x 512 tiles = 65536 px, far beyond anything the
/// game ever stores, because a set is sparse. What a single page file holds is the bounding box of
/// its own tiles, and that box times a tile's content area is exactly the DDS size the extractor
/// writes out (`infrastructure::export` sizes it the same way), so that is what is reported here.
///
/// The bytes are parsed here rather than through `GtsFile`: that type exposes the per-tile content
/// size but keeps its tile tables crate-private, and a detail view must not stage anything to disk.
/// `sizes` caches by GTS path, since one GTS serves every page file of its tile set.
pub fn page_file_size(
    pak: &mut Archives,
    sizes: &mut HashMap<String, PageFileSizes>,
    matched: &GtpMatch,
    hash: &str,
) -> Option<(u32, u32)> {
    let gts_rel = derive_gts_path(&matched.gtp_path);
    if !sizes.contains_key(&gts_rel) {
        let parsed = pak
            .read_from(Pak::VirtualTextures, &gts_rel)
            .ok()
            .and_then(|bytes| parse_page_file_sizes(&bytes))
            .unwrap_or_default();
        sizes.insert(gts_rel.clone(), parsed);
    }

    // Page files are looked up by substring, the way maclarian resolves them
    // (`GtsFile::find_page_file_index`), so the names accepted here are exactly the ones the
    // extractor accepts
    sizes
        .get(&gts_rel)?
        .iter()
        .find(|(name, _)| name.contains(hash))
        .and_then(|(_, size)| *size)
}

/// Page file sizes out of a GTS buffer: one tile bounding box per page file, layer 0 only.
///
/// Field offsets follow maclarian's own readers (`gts::read_header` / `gts::read_sections`):
/// little endian throughout, `num_levels` at byte 40, the tile geometry at 52..64, the flat tile
/// info table at 68..80 (12 bytes per record) and the packed tile ids at 88..100 (4 bytes per
/// record), the page file table at 132..144.
fn parse_page_file_sizes(data: &[u8]) -> Option<PageFileSizes> {
    if u32::from_le_bytes(read_le(data, 0)?) != GTS_MAGIC {
        return None;
    }

    let num_levels = u32::from_le_bytes(read_le(data, 40)?) as usize;
    let tile_width = i32::from_le_bytes(read_le(data, 52)?);
    let tile_height = i32::from_le_bytes(read_le(data, 56)?);
    let tile_border = i32::from_le_bytes(read_le(data, 60)?);
    let num_flat_tiles = u32::from_le_bytes(read_le(data, 68)?) as usize;
    let flat_tiles_offset = u64::from_le_bytes(read_le(data, 72)?) as usize;
    let num_packed_tiles = u32::from_le_bytes(read_le(data, 88)?) as usize;
    let packed_tiles_offset = u64::from_le_bytes(read_le(data, 92)?) as usize;
    let num_page_files = u32::from_le_bytes(read_le(data, 132)?) as usize;
    let page_files_offset = u64::from_le_bytes(read_le(data, 136)?) as usize;

    // A tile carries its neighboring pixels in the border on each side; the texture itself is the
    // content area only, which is what the extractor writes out
    let content_width = (tile_width - tile_border * 2) as u32;
    let content_height = (tile_height - tile_border * 2) as u32;
    if num_levels == 0
        || num_levels > MAX_MIP_LEVELS
        || num_page_files > MAX_PAGE_FILES
        || content_width == 0
        || content_height == 0
    {
        return None;
    }

    // Bounding box per (page file, mip level), `[min_x, max_x, min_y, max_y]` in tile units
    let mut bounds: Vec<Option<[u16; 4]>> = vec![None; num_page_files * num_levels];

    for index in 0..num_flat_tiles {
        let record = flat_tiles_offset + index * FLAT_TILE_INFO_SIZE;
        let page_file = u16::from_le_bytes(read_le(data, record)?) as usize;
        // The packed tile id closes the record, after page, chunk and one more word
        let packed_index = u32::from_le_bytes(read_le(data, record + 8)?) as usize;
        if page_file >= num_page_files || packed_index >= num_packed_tiles {
            continue;
        }
        let packed = u32::from_le_bytes(read_le(data, packed_tiles_offset + packed_index * 4)?);

        // Layer 0 only: the albedo page, and one size is all a detail row shows
        if packed & PACKED_LAYER_MASK != 0 {
            continue;
        }
        let level = ((packed >> 4) & PACKED_LEVEL_MASK) as usize;
        if level >= num_levels {
            continue;
        }
        let x = (packed >> 20) as u16;
        let y = ((packed >> 8) & PACKED_ROW_MASK) as u16;
        let entry = bounds[page_file * num_levels + level].get_or_insert([x, x, y, y]);
        entry[0] = entry[0].min(x);
        entry[1] = entry[1].max(x);
        entry[2] = entry[2].min(y);
        entry[3] = entry[3].max(y);
    }

    let mut sizes = Vec::with_capacity(num_page_files);
    for page_file in 0..num_page_files {
        // A truncated name only costs that row its size, so it is not a parse failure
        let name = read_page_file_name(data, page_files_offset + page_file * PAGE_FILE_RECORD_SIZE)
            .unwrap_or_default();
        // The highest resolution that page file has: maclarian picks the levels the same way, since
        // a page file does not have to store level 0 (material maps are often stored lower)
        let size = (0..num_levels)
            .filter_map(|level| bounds[page_file * num_levels + level].map(|box_| (level, box_)))
            .min_by_key(|(level, _)| *level)
            .map(|(_, box_)| {
                let width = (box_[1] - box_[0] + 1) as u32 * content_width;
                let height = (box_[3] - box_[2] + 1) as u32 * content_height;
                (width, height)
            })
            .filter(|(width, height)| *width > 0 && *height > 0);
        sizes.push((name, size));
    }

    Some(sizes)
}

/// UTF-16LE page file name out of a GTS page file record, up to its first NUL
fn read_page_file_name(data: &[u8], offset: usize) -> Option<String> {
    let raw = data.get(offset..offset.checked_add(PAGE_FILE_NAME_SIZE)?)?;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .take_while(|unit| *unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// Flat tile info record: `page_file_index` first, `packed_tile_id_index` at byte 8
const FLAT_TILE_INFO_SIZE: usize = 12;

/// Page file record: a 256-character UTF-16LE name, then page count, GUID and one more word
const PAGE_FILE_RECORD_SIZE: usize = 536;
const PAGE_FILE_NAME_SIZE: usize = 512;

/// Ceiling on the page file count of one tile set. The game's largest sets hold a few hundred; the
/// cap only exists so a corrupt header cannot be turned into a huge allocation.
const MAX_PAGE_FILES: usize = 1 << 14;

/// Ceiling on the mip level count: a packed tile id stores its level in 4 bits
const MAX_MIP_LEVELS: usize = 16;

/// Bit fields of a packed tile id (`GtsPackedTileId::from_u32`): layer, level, row, column
const PACKED_LAYER_MASK: u32 = 0b1111;
const PACKED_LEVEL_MASK: u32 = 0b1111;
const PACKED_ROW_MASK: u32 = 0xFFF;

/// `GtsHeader::MAGIC` ('GRPG'), which maclarian keeps crate-private
const GTS_MAGIC: u32 = 0x4750_5247;

/// Fixed-size little-endian read; `None` when the buffer is too short (truncated or not a GTS)
fn read_le<const N: usize>(data: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    data.get(offset..end)?.try_into().ok()
}

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
        .map(|vt| {
            let mut summary =
                VirtualTextureSummary::new(vt, match_for_hash(matches, &vt.gtex_hash));
            summary.source = source_of(sources, &vt.id);
            summary.parameter_name =
                virtual_texture_parameter(&vt.id, &value.material_ids, materials);
            summary
        })
        .collect()
}

/// Resolve GTS candidates and stage them into the `shared` directory, ordered by likelihood:
/// 1. `<GTP with the `_<hash>` suffix stripped>.gts` — the tile set's own GTS (maclarian's standard
///    derivation), the only candidate that is normally needed
/// 2. `.gts` files in the same directory whose name is a prefix of the GTP file name (fallback for
///    mismatched GTS/GTP naming); the longer the GTS name, the higher the priority
///
/// More than one candidate is staged because a GTS outlives the page file that led here — tile sets
/// do not always share the index spelling of their page files — and `infrastructure::export` tries
/// them in order until one accepts the page file. Staged files are reused by file name, so the GTS
/// of a tile set is read once no matter how many of its page files this export touches.
fn stage_gts_candidates(
    pak: &mut Archives,
    matched: &GtpMatch,
    shared: &Path,
) -> Result<Vec<PathBuf>, String> {
    let gtp_rel = matched.gtp_path.as_str();
    let gts_rel = derive_gts_path(gtp_rel);

    let gtp_stem = Path::new(gtp_rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let gtp_dir = Path::new(gtp_rel)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut staged: Vec<PathBuf> = Vec::new();

    // 1. Standard derived name
    if stage_gts_file(pak, &gts_rel, shared, &mut staged) {
        return Ok(staged);
    }

    // 2. Same-directory prefix fallback: the tile set names of the virtual texture archive, filtered
    //    to the GTP's directory and a name prefix
    let mut fallbacks: Vec<String> = pak
        .list_in(Pak::VirtualTextures)?
        .into_iter()
        .filter(|p| {
            p.to_lowercase().ends_with(".gts")
                && Path::new(p)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    == gtp_dir
        })
        .filter(|p| {
            let stem = Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_lowercase();
            !stem.is_empty() && gtp_stem.starts_with(&stem)
        })
        .collect();
    // The closer the GTS name is to the GTP name, the more likely it hits; try in descending
    // file-name length
    fallbacks.sort_by_key(|p| std::cmp::Reverse(p.len()));

    for rel in fallbacks {
        stage_gts_file(pak, &rel, shared, &mut staged);
    }

    if staged.is_empty() {
        return Err(format!(
            "{gts_rel} not found in {}",
            Pak::VirtualTextures.file_name()
        ));
    }
    Ok(staged)
}

/// Stage one GTS to disk; reuse it directly when already staged (shared with another GTP)
fn stage_gts_file(pak: &mut Archives, rel: &str, shared: &Path, staged: &mut Vec<PathBuf>) -> bool {
    let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let path = shared.join(name);
    if path.exists() {
        staged.push(path);
        return true;
    }
    match pak.read_from(Pak::VirtualTextures, rel) {
        Ok(bytes) => match fs::write(&path, &bytes) {
            Ok(()) => {
                staged.push(path);
                true
            }
            Err(_) => false,
        },
        Err(_) => false,
    }
}

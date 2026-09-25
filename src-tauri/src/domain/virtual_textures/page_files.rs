//! The page files behind a virtual texture: the pixel size of a page file, read out of the GTS
//! metadata of its tile set.

use std::collections::HashMap;

use maclarian::merged::GtpMatch;

use crate::domain::naming::derive_gts_path;
use crate::infrastructure::archives::{Archives, Pak};

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

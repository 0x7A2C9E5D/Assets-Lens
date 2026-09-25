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
    cache_page_file_sizes(pak, sizes, &gts_rel);

    // Page files are looked up by substring, the way maclarian resolves them
    // (`GtsFile::find_page_file_index`), so the names accepted here are exactly the ones the
    // extractor accepts
    sizes
        .get(&gts_rel)?
        .iter()
        .find(|(name, _)| name.contains(hash))
        .and_then(|(_, size)| *size)
}

/// Parse the page file table of one tile set into `sizes`, unless it is already cached there. One
/// GTS serves every page file of its tile set, so it is read at most once per path.
fn cache_page_file_sizes(
    pak: &mut Archives,
    sizes: &mut HashMap<String, PageFileSizes>,
    gts_rel: &str,
) {
    if sizes.contains_key(gts_rel) {
        return;
    }
    let parsed = pak
        .read_from(Pak::VirtualTextures, gts_rel)
        .ok()
        .and_then(|bytes| parse_page_file_sizes(&bytes))
        .unwrap_or_default();
    sizes.insert(gts_rel.to_string(), parsed);
}

/// Page file sizes out of a GTS buffer: one tile bounding box per page file, layer 0 only.
///
/// The bytes are read here rather than through `GtsFile`: that type exposes the per-tile content
/// size but keeps its tile tables crate-private, and a detail view must not stage anything to disk.
fn parse_page_file_sizes(data: &[u8]) -> Option<PageFileSizes> {
    let header = parse_gts_header(data)?;
    let bounds = tile_bounds(data, &header)?;
    Some(page_file_table(data, &header, &bounds))
}

/// The fixed header fields the page file table is read with
struct GtsHeader {
    num_levels: usize,
    tile_width: i32,
    tile_height: i32,
    tile_border: i32,
    num_flat_tiles: usize,
    flat_tiles_offset: usize,
    num_packed_tiles: usize,
    packed_tiles_offset: usize,
    num_page_files: usize,
    page_files_offset: usize,
}

impl GtsHeader {
    /// Pixel size of one tile's content area. A tile carries its neighboring pixels in the border on
    /// each side; the content area is the texture itself, which is what the extractor writes out.
    fn content_size(&self) -> (u32, u32) {
        (
            (self.tile_width - self.tile_border * 2) as u32,
            (self.tile_height - self.tile_border * 2) as u32,
        )
    }

    /// Whether the header is plausible enough to size an allocation from it
    fn is_plausible(&self) -> bool {
        let (width, height) = self.content_size();
        self.num_levels > 0
            && self.num_levels <= MAX_MIP_LEVELS
            && self.num_page_files <= MAX_PAGE_FILES
            && width > 0
            && height > 0
    }
}

/// The header of a GTS buffer, magic checked and bounds validated: `None` when the buffer is not a
/// GTS, or states a size that cannot be trusted
fn parse_gts_header(data: &[u8]) -> Option<GtsHeader> {
    if u32::from_le_bytes(read_le(data, 0)?) != GTS_MAGIC {
        return None;
    }
    let header = read_gts_header(data)?;
    header.is_plausible().then_some(header)
}

/// Header fields at their fixed offsets. Offsets follow maclarian's own readers
/// (`gts::read_header` / `gts::read_sections`): little endian throughout, `num_levels` at byte 40,
/// the tile geometry at 52..64, the flat tile info table at 68..80 (12 bytes per record) and the
/// packed tile ids at 88..100 (4 bytes per record), the page file table at 132..144.
///
/// The fields are read one by one because that is what the format is: the offsets are the only thing
/// giving them meaning, so splitting the reads further would separate each offset from its name.
fn read_gts_header(data: &[u8]) -> Option<GtsHeader> {
    Some(GtsHeader {
        num_levels: u32::from_le_bytes(read_le(data, 40)?) as usize,
        tile_width: i32::from_le_bytes(read_le(data, 52)?),
        tile_height: i32::from_le_bytes(read_le(data, 56)?),
        tile_border: i32::from_le_bytes(read_le(data, 60)?),
        num_flat_tiles: u32::from_le_bytes(read_le(data, 68)?) as usize,
        flat_tiles_offset: u64::from_le_bytes(read_le(data, 72)?) as usize,
        num_packed_tiles: u32::from_le_bytes(read_le(data, 88)?) as usize,
        packed_tiles_offset: u64::from_le_bytes(read_le(data, 92)?) as usize,
        num_page_files: u32::from_le_bytes(read_le(data, 132)?) as usize,
        page_files_offset: u64::from_le_bytes(read_le(data, 136)?) as usize,
    })
}

/// Bounding box per (page file, mip level), `[min_x, max_x, min_y, max_y]` in tile units, over the
/// layer 0 tiles of the flat tile info table
fn tile_bounds(data: &[u8], header: &GtsHeader) -> Option<Vec<Option<[u16; 4]>>> {
    let mut bounds: Vec<Option<[u16; 4]>> = vec![None; header.num_page_files * header.num_levels];
    for (page_file, packed) in read_flat_tiles(data, header)? {
        if let Some((index, x, y)) = flat_tile(page_file, packed, header) {
            widen(bounds[index].get_or_insert([x, x, y, y]), x, y);
        }
    }
    Some(bounds)
}

/// The page file and packed tile id of every flat tile info record, in table order. A record pointing
/// outside the tables names no tile and is dropped; a table running past the end of the buffer is no
/// GTS and fails the parse.
fn read_flat_tiles(data: &[u8], header: &GtsHeader) -> Option<Vec<(usize, u32)>> {
    let mut records = Vec::with_capacity(header.num_flat_tiles);
    for index in 0..header.num_flat_tiles {
        let base = header.flat_tiles_offset + index * FLAT_TILE_INFO_SIZE;
        let page_file = u16::from_le_bytes(read_le(data, base)?) as usize;
        // The packed tile id closes the record, after page, chunk and one more word
        let packed_index = u32::from_le_bytes(read_le(data, base + 8)?) as usize;
        if page_file >= header.num_page_files || packed_index >= header.num_packed_tiles {
            continue;
        }
        let offset = header.packed_tiles_offset + packed_index * 4;
        records.push((page_file, u32::from_le_bytes(read_le(data, offset)?)));
    }
    Some(records)
}

/// The layer 0 tile a flat tile info record names: the bounds row it widens, and its coordinates.
/// `None` for a tile of another layer — layer 0 is the albedo page, the only one a size is shown for
/// — or of a mip level this tile set does not have.
fn flat_tile(page_file: usize, packed: u32, header: &GtsHeader) -> Option<(usize, u16, u16)> {
    if packed & PACKED_LAYER_MASK != 0 {
        return None;
    }
    let level = ((packed >> 4) & PACKED_LEVEL_MASK) as usize;
    if level >= header.num_levels {
        return None;
    }
    let index = page_file * header.num_levels + level;
    Some((
        index,
        (packed >> 20) as u16,
        ((packed >> 8) & PACKED_ROW_MASK) as u16,
    ))
}

/// Grow a bounding box so it covers one more tile
fn widen(entry: &mut [u16; 4], x: u16, y: u16) {
    entry[0] = entry[0].min(x);
    entry[1] = entry[1].max(x);
    entry[2] = entry[2].min(y);
    entry[3] = entry[3].max(y);
}

/// One entry per page file, in table order
fn page_file_table(data: &[u8], header: &GtsHeader, bounds: &[Option<[u16; 4]>]) -> PageFileSizes {
    let mut sizes = Vec::with_capacity(header.num_page_files);
    for page_file in 0..header.num_page_files {
        let offset = header.page_files_offset + page_file * PAGE_FILE_RECORD_SIZE;
        // A truncated name only costs that row its size, so it is not a parse failure
        let name = read_page_file_name(data, offset).unwrap_or_default();
        sizes.push((name, stored_size(bounds, header, page_file)));
    }
    sizes
}

/// The highest resolution one-page file stores, in pixels; `None` when it holds no layer 0 tile.
/// maclarian picks the levels the same way, since a page file does not have to store level 0
/// (material maps are often stored lower).
fn stored_size(
    bounds: &[Option<[u16; 4]>],
    header: &GtsHeader,
    page_file: usize,
) -> Option<(u32, u32)> {
    let levels = header.num_levels;
    let (_, box_) = (0..levels)
        .filter_map(|level| bounds[page_file * levels + level].map(|box_| (level, box_)))
        .min_by_key(|(level, _)| *level)?;
    let (content_width, content_height) = header.content_size();
    let width = (box_[1] - box_[0] + 1) as u32 * content_width;
    let height = (box_[3] - box_[2] + 1) as u32 * content_height;
    (width > 0 && height > 0).then_some((width, height))
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

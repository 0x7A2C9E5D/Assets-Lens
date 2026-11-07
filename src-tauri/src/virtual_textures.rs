//! Virtual texture page files: stage the page file a `GtpMatch` names plus the GTS that covers it.
//!
//! A GTS is the metadata of a whole tile set and serves many page files. Both live in the archives
//! as raw blocks while `VirtualTextureExtractor` takes file paths, so they are staged to disk here;
//! `export.rs` turns the staged paths into artifacts.

use std::fs;
use std::path::{Path, PathBuf};

use maclarian::merged::GtpMatch;

use crate::archives::{Archives, Pak};

/// Files staged for one-page file, both already on disk
pub struct StagedSources {
    /// Page file (`.gtp`), named after its archive entry
    pub gtp: PathBuf,
    /// Candidate GTS files, ordered by likelihood; normally the first one hits.
    pub gts_candidates: Vec<PathBuf>,
}

/// Stage the page file `matched` names plus the GTS that covers it into `shared`; staged files are
/// reused by name, so one GTS is read once for all of its page files.
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
/// Layer 0 only; `None` for a page file that stores no layer 0 tiles.
pub type PageFileSizes = Vec<(String, Option<(u32, u32)>)>;

/// Page file sizes by GTS path. One GTS serves every page file of its tile set, so the tiles read
/// for one detail view would otherwise be read again for the next one. Bounded, because the cache
/// grows with every asset the user browses.
pub type PageFileSizeCache = moka::sync::Cache<String, PageFileSizes>;

/// Upper bound on cached tile sets; each entry is the page file list of one GTS (tens of rows)
pub const PAGE_FILE_SIZE_CACHE_CAPACITY: u64 = 2048;

/// Pixel size of the page file `hash` names, in the resolution that page file really stores.
///
/// Level 0's grid describes the whole sparse tile set, so the size is the bounding box of the page
/// file's own tiles times a tile's content area — exactly the DDS the extractor writes. The bytes
/// are parsed here because `GtsFile` keeps its tile tables crate-private.
pub fn page_file_size(
    pak: &mut Archives,
    sizes: &PageFileSizeCache,
    matched: &GtpMatch,
    hash: &str,
) -> Option<(u32, u32)> {
    let gts_rel = derive_gts_path(&matched.gtp_path);
    let size_list = match sizes.get(&gts_rel) {
        Some(cached) => cached,
        None => {
            let parsed = pak
                .read_from(Pak::VirtualTextures, &gts_rel)
                .ok()
                .and_then(|bytes| parse_page_file_sizes(&bytes))
                .unwrap_or_default();
            // Inserted even when parsing failed: "no sizes here" is worth caching
            sizes.insert(gts_rel.clone(), parsed.clone());
            parsed
        }
    };

    // Page files are looked up by substring, the way `GtsFile::find_page_file_index` resolves them
    size_list
        .iter()
        .find(|(name, _)| name.contains(hash))
        .and_then(|(_, size)| *size)
}

/// Page file sizes out of a GTS buffer: one tile bounding box per page file, layer 0 only.
///
/// Offsets follow maclarian's readers: `num_levels` at 40, tile geometry at 52..64, flat tiles at
/// 68..80 (12 bytes per record), packed ids at 88..100 (4 bytes), page file table at 132..144.
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
        // The highest resolution that page file has; a page file need not store level 0
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

/// Ceiling on the page file count of one tile set: the game's largest sets hold a few hundred, so
/// the cap only stops a corrupt header becoming a huge allocation.
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

/// GTP path → GTS path: strip the trailing `_<32 hex digits>` and swap the extension
/// (`..._5_<hash>.gtp` → `..._5.gts`). Every page file of a tile set derives the same GTS.
fn derive_gts_path(gtp_path: &str) -> String {
    // Only the file name loses its hash suffix; the directory has to survive into the result
    let (dir, name) = gtp_path.rsplit_once('/').unwrap_or(("", gtp_path));
    let stem = name
        .strip_suffix(".gtp")
        .or_else(|| name.strip_suffix(".GTP"))
        .unwrap_or(name);

    let stripped = stem.rfind('_').filter(|pos| {
        let suffix = &stem[pos + 1..];
        suffix.len() == 32 && suffix.chars().all(|c| c.is_ascii_hexdigit())
    });
    let stem = stripped.map_or(stem, |pos| &stem[..pos]);

    if dir.is_empty() {
        format!("{stem}.gts")
    } else {
        format!("{dir}/{stem}.gts")
    }
}

/// Resolve GTS candidates and stage them into `shared`, ordered by likelihood:
/// 1. `<GTP with the hash suffix stripped>.gts`, the tile set's own GTS
/// 2. same-directory `.gts` files whose name prefixes the GTP name, longest first
///
/// `export.rs` tries them in order until one accepts the page file.
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

    // 2. Same-directory prefix fallback, longest GTS name first
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
    // The closer the GTS name is to the GTP name, the more likely it hits
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

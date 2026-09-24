use std::collections::HashMap;

use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use serde::{Deserialize, Serialize};

use crate::domain::material::{virtual_texture_parameter, MaterialInfo};
use crate::domain::source::{source_of, ModSources};

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub percent: f32,
}

/// Texture reference (DDS).
///
/// A row of the asset's texture list (the detail panel), and — copied — the entry the export manifest
/// material binding it carries: the resource stated in full (name, path, size, parameter) rather than
/// a GUID to look up. Hence `Clone`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    /// Mod providing this texture; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
}

impl From<&TextureRef> for TextureSummary {
    fn from(value: &TextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            // Settled by the caller, which is the only place holding the source map
            source: None,
            path: value.dds_path.clone(),
            width: value.width,
            height: value.height,
            parameter_name: value.parameter_name.clone(),
        }
    }
}

/// Which list of the asset a material binding points into
#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingKind {
    /// A regular texture (`TextureSummary` row)
    Texture,
    /// A streaming virtual texture (`VirtualTextureSummary` row)
    Virtual,
}

/// One resource a material binds, as the detail panel lists it under that material.
///
/// The relation is stated per material and keyed by GUID: a material name is not unique, so inverting
/// a texture list by name mixes up same-named materials and drops the bindings of an unnamed one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialBinding {
    pub kind: BindingKind,
    /// GUID of the resource, the same value its row in the asset's lists carries
    pub id: String,
    /// Name of that row; empty when the resource has none, and the panel falls back to the GUID
    pub name: String,
    /// Parameter the binding fills (e.g. `virtualtexture`), for a virtual texture. Taken from this
    /// material's own binding rather than the resource: the name belongs to the binding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
}

/// Material reference. The GUID stays the identity (names are not unique), the name is what makes
/// a material row readable, and the template file is where the material is defined.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSummary {
    pub id: String,
    /// Human-readable name from `MaterialBank`; empty when the material is unknown, in which case
    /// the detail panel falls back to the GUID
    pub name: String,
    /// Mod providing this material; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// The resources this material binds, in binding order. Left out of the JSON while empty, which
    /// is what a material the cache does not know yields.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<MaterialBinding>,
}

impl MaterialSummary {
    /// `known` is the name cache entry for this GUID; an unknown material keeps an empty name
    /// rather than dropping the row. The bindings are filled later, once the asset's resource rows
    /// exist to read them off (see `fill_material_bindings`).
    fn new(id: &str, known: Option<&MaterialInfo>, sources: &ModSources) -> Self {
        Self {
            id: id.to_string(),
            name: known
                .map(|material| material.name.clone())
                .unwrap_or_default(),
            source: source_of(sources, id),
            source_file: known
                .map(|material| material.source_file.clone())
                .unwrap_or_default(),
            bindings: Vec::new(),
        }
    }
}

/// Summaries of `material_ids` in the order the asset lists them.
///
/// The material rows of the detail panel; the export manifest lists the same identity plus the
/// resources each material binds (see `ExportMaterial`). A material is identified by its GUID but is
/// only readable through the name, and a list carrying bare GUIDs cannot be looked up in the game
/// data. A material the cache does not know keeps an empty name instead of dropping out of the list,
/// so the reference itself is never lost.
pub fn material_summaries(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
    sources: &ModSources,
) -> Vec<MaterialSummary> {
    material_ids
        .iter()
        .map(|id| MaterialSummary::new(id, materials.get(id), sources))
        .collect()
}

/// Fill in, for every material row, the resources of the asset that material binds.
///
/// A material names its resources by GUID (see `MaterialInfo`), so each one is looked up in the
/// asset's own rows — the same rows the detail panel lists beside the material. Both sides are joined
/// on the GUID rather than on the name, which is what keeps two same-named materials apart and lets an
/// unnamed one still show what it binds. A resource a row does not list contributes nothing, and a
/// material the cache does not know keeps an empty binding list.
fn fill_material_bindings(
    materials: &mut [MaterialSummary],
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    known: &HashMap<String, MaterialInfo>,
) {
    let textures: HashMap<&str, &TextureSummary> =
        textures.iter().map(|row| (row.id.as_str(), row)).collect();
    let virtual_textures: HashMap<&str, &VirtualTextureSummary> = virtual_textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();

    for row in materials.iter_mut() {
        let Some(material) = known.get(&row.id) else {
            continue;
        };

        row.bindings = material
            .texture_ids
            .iter()
            .filter_map(|id| textures.get(id.as_str()))
            .map(|texture| MaterialBinding {
                kind: BindingKind::Texture,
                id: texture.id.clone(),
                name: texture.name.clone(),
                parameter_name: texture.parameter_name.clone(),
            })
            .chain(material.virtual_textures.iter().filter_map(|binding| {
                let row = virtual_textures.get(binding.id.as_str())?;
                Some(MaterialBinding {
                    kind: BindingKind::Virtual,
                    id: row.id.clone(),
                    name: row.name.clone(),
                    // Taken from this material's own binding: the parameter belongs to the binding,
                    // not to the resource, so the row-level one would only be an approximation
                    parameter_name: Some(binding.parameter_name.clone())
                        .filter(|name| !name.is_empty()),
                })
            }))
            .collect();
    }
}

/// One material as `asset.json` lists it: the identity of a `MaterialSummary` plus the resources the
/// material binds, each stated in full.
///
/// A material row carries the asset's texture rows for its own resources, in binding order: each
/// resource in full (name, path, size, parameter) rather than a GUID to look up, so a reader takes the
/// resources of a material from one place instead of joining two lists by GUID. The inverse relation
/// (`materialNames`) is the one field left out of those copies — a material containing its textures
/// says that already.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMaterial {
    pub id: String,
    pub name: String,
    /// Mod providing this material; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub source_file: String,
    /// The textures this material binds, in parameter order
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub textures: Vec<TextureSummary>,
    /// The virtual textures this material binds, in binding order
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl ExportMaterial {
    /// `known` is the name cache entry for this GUID. The identity is taken from `MaterialSummary`, so
    /// an unknown material keeps an empty name here exactly as it does on a detail row.
    ///
    /// A material names its resources by GUID, so `textures` / `virtual_textures` are looked up in the
    /// asset's own rows by that GUID.
    fn new(
        id: &str,
        known: Option<&MaterialInfo>,
        textures: &HashMap<&str, &TextureSummary>,
        virtual_textures: &HashMap<&str, &VirtualTextureSummary>,
        sources: &ModSources,
    ) -> Self {
        let MaterialSummary {
            id,
            name,
            source,
            source_file,
            // The manifest states the resources inside each material, so the row-level bindings of
            // the detail panel have nothing to add here
            bindings: _,
        } = MaterialSummary::new(id, known, sources);

        Self {
            id,
            name,
            source,
            source_file,
            // In the material's own order (its parameter order); an id the asset's texture rows do
            // not carry contributes nothing
            textures: known
                .map(|material| {
                    material
                        .texture_ids
                        .iter()
                        .filter_map(|id| textures.get(id.as_str()))
                        .map(|row| (*row).clone())
                        .collect()
                })
                .unwrap_or_default(),
            virtual_textures: known
                .map(|material| {
                    material
                        .virtual_textures
                        .iter()
                        .filter_map(|binding| virtual_textures.get(binding.id.as_str()))
                        .map(|row| (*row).clone())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// The materials of `value` in the asset's own order, as the export manifest lists them: each one
/// carrying the asset's texture and virtual texture rows for the resources it binds.
///
/// Those rows are the manifest's only statement of the asset's resources — the material containing them
/// says which material they belong to, so a separate list beside the materials would only repeat it.
/// This is also why the rows are whole: the manifest is read on its own, so a GUID to look up would
/// not do.
pub fn manifest_materials(
    value: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
    sources: &ModSources,
) -> Vec<ExportMaterial> {
    let textures: HashMap<&str, &TextureSummary> =
        textures.iter().map(|row| (row.id.as_str(), row)).collect();
    let virtual_textures: HashMap<&str, &VirtualTextureSummary> = virtual_textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();

    value
        .material_ids
        .iter()
        .map(|id| ExportMaterial::new(id, materials.get(id), &textures, &virtual_textures, sources))
        .collect()
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
    /// and an export both read up front (`commands::ensure_virtual_texture_parameters`).
    ///
    /// One name per row, so an asset that binds a virtual texture through several materials with
    /// different parameters shows only the first here; each material states its own name in its
    /// `bindings` (see `fill_material_bindings`).
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

/// The textures of `value`, in the asset's own order.
///
/// The texture list of the detail panel, and the source the export manifest builds its material rows
/// from (see `manifest_materials`). Which material binds a texture is stated the other way round, on
/// the material rows (see `fill_material_bindings`).
pub fn texture_summaries(value: &VisualAsset, sources: &ModSources) -> Vec<TextureSummary> {
    value
        .textures
        .iter()
        .map(|texture| {
            let mut summary = TextureSummary::from(texture);
            summary.source = source_of(sources, &texture.id);
            summary
        })
        .collect()
}

/// The virtual textures of `value`, in the asset's own order, each carrying the parameter the binding
/// fills.
///
/// The virtual texture list of the detail panel, and the source the export manifest builds its material
/// rows from (see `manifest_materials`). The parameter name is only ever filled once the caller
/// resolved it — the templates carrying it are read by a detail view and by an export
/// (`commands::ensure_virtual_texture_parameters`), not by this function — so a row stays without one
/// until then. Which material binds a virtual texture is likewise stated on the material rows (see
/// `fill_material_bindings`).
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

/// Full visual asset information (detail panel)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualAssetDetail {
    /// Visual resource ID (GUID) — the lookup key, since names are not unique
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    pub materials: Vec<MaterialSummary>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl VisualAssetDetail {
    /// `matches` are the page files resolved for this asset's virtual textures; pass an empty
    /// slice to skip the lookup (the rows then show the hash without a page file).
    /// `materials` is the name cache built with the database: it labels the material rows and lists,
    /// per material, the resources of this asset that material binds (see `fill_material_bindings`).
    /// `sources` labels every row with the mod that provides it, and leaves the game's own rows
    /// unlabeled.
    pub fn new(
        value: &VisualAsset,
        matches: &[GtpMatch],
        materials: &HashMap<String, MaterialInfo>,
        sources: &ModSources,
    ) -> Self {
        // Built in this order because the material rows end up listing their own resources: the
        // bindings are read off the two resource lists, so those have to exist first
        let mut material_rows = material_summaries(&value.material_ids, materials, sources);
        let textures = texture_summaries(value, sources);
        let virtual_textures = virtual_texture_summaries(value, matches, materials, sources);
        fill_material_bindings(&mut material_rows, &textures, &virtual_textures, materials);

        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            source: source_of(sources, &value.id),
            path: value.gr2_path.clone(),
            materials: material_rows,
            textures,
            virtual_textures,
        }
    }
}

/// 3D preview payload: the GR2 mesh converted to GLB.
/// Stored as a Base64 string instead of `Vec<u8>`: a byte vector would serialize through serde into
/// a JSON array of numbers (one per byte), inflating a multi-MB model to tens of MB of JSON;
/// Base64 only grows by ~33% and keeps everything in memory without temp files
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreview {
    pub base64: String,
}

/// Visual asset list item: highlights the composition of the visual itself
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualSummary {
    /// Visual resource ID (GUID): the row identity — one name can belong to several visuals, so the
    /// list cannot be keyed by name
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

impl VisualSummary {
    pub fn of(value: &VisualAsset, sources: &ModSources) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            source: source_of(sources, &value.id),
            material_count: value.material_ids.len(),
            texture_count: value.textures.len(),
            virtual_texture_count: value.virtual_textures.len(),
        }
    }
}

/// Database statistics
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    pub visual_count: usize,
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

/// What the session knows about the index persisted on disk, so the Database page can say where its
/// statistics came from.
///
/// `state` is one of:
/// - `idle` — nothing to report: no directory selected yet, or the index was built in this session
/// - `loaded` — the index was read back from disk; `builtAt` is the unix time it was built at
/// - `stale` — a file is there but was refused; `code` says why
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatus {
    pub state: String,
    /// Unix seconds the persisted index was built at (only for `loaded`)
    pub built_at: Option<u64>,
    /// Stable reason a file was refused, for the frontend to map to copy: one of `version`,
    /// `game_paks`, `mod_paks`, `unreadable`
    pub code: Option<String>,
    /// Raw detail behind a `code` that needs one: the versions that wrote the file, or the parse error
    pub detail: Option<String>,
}

impl CacheStatus {
    /// Nothing to report
    pub fn idle() -> Self {
        Self {
            state: "idle".to_string(),
            built_at: None,
            code: None,
            detail: None,
        }
    }

    /// The index came back from disk
    pub fn loaded(built_at: u64) -> Self {
        Self {
            state: "loaded".to_string(),
            built_at: Some(built_at),
            code: None,
            detail: None,
        }
    }

    /// A persisted index was found but could not be used
    pub fn stale(code: &str, detail: Option<String>) -> Self {
        Self {
            state: "stale".to_string(),
            built_at: None,
            code: Some(code.to_string()),
            detail,
        }
    }
}

/// Everything a page needs to render without asking anything else: the game data directory this
/// session works on, the statistics of the index held for it, and where that index came from.
///
/// Returned once at startup (`commands::restore_state`) — the frontend used to assemble the same three
/// facts itself, from Web storage plus a comparison against the backend's directory.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// The game data directory in use, or `None` while none is set
    pub game_path: Option<String>,
    /// Statistics of the index held, or `None` when nothing is built yet
    pub stats: Option<DatabaseStats>,
    pub cache: CacheStatus,
}

/// App metadata for the About page
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Application version (compile-time, kept in sync with tauri.conf.json)
    pub version: String,
}

/// Generic pagination result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
}

/* ---------- Asset export ---------- */

/// Mesh export format: the raw GR2 out of the archive, or a converted GLB
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MeshFormat {
    /// Raw GR2 bytes, straight out of the PAK (no conversion)
    #[default]
    Gr2,
    /// Converted GLB mesh; textures are exported separately
    Glb,
}

impl MeshFormat {
    pub fn is_glb(self) -> bool {
        matches!(self, Self::Glb)
    }

    /// File extension of the mesh artifact
    pub fn extension(self) -> &'static str {
        match self {
            Self::Gr2 => "gr2",
            Self::Glb => "glb",
        }
    }
}

/// Texture export format: no separate textures, DDS, or PNG.
/// Applies to both regular textures and virtual textures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextureFormat {
    /// Do not export texture files separately
    None,
    /// Export as DDS (keep the archive format)
    #[default]
    Dds,
    /// Export as PNG (discard DDS after successful conversion)
    Png,
}

impl TextureFormat {
    pub fn is_export(self) -> bool {
        !matches!(self, Self::None)
    }

    pub fn is_png(self) -> bool {
        matches!(self, Self::Png)
    }
}

/// Export options (submitted from the frontend)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Mesh output format: raw GR2 or converted GLB
    #[serde(default)]
    pub mesh_format: MeshFormat,
    /// Texture output format for separate files — covers both regular textures and virtual textures
    #[serde(default)]
    pub texture_format: TextureFormat,
}

/// Export progress, pushed through a Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// Phase: prepare / model / textures / virtualTextures / manifest / done
    pub phase: String,
    pub current_file: Option<String>,
    pub percent: f32,
}

/// One exported artifact
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    /// Artifact kind: gr2 / glb / dds / png
    pub kind: String,
    pub size_bytes: usize,
}

/// Export warning: `code` is a stable enum (mapped to i18n copy on the frontend) while `detail`
/// carries the raw detail (path / error); the frontend shows `detail` directly for unknown codes,
/// so warnings passed through from maclarian never end up as a missing-copy gap
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub code: String,
    pub detail: String,
}

/// Export result
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// The directory actually written to (destDir/<asset name>)
    pub output_dir: String,
    pub files: Vec<ExportedFile>,
    /// Non-fatal issues: missing textures, failed PNG conversions, etc.
    pub warnings: Vec<ExportWarning>,
}

/// Content of asset.json: the asset and the resources it is made of — not a listing of the export
/// directory. Which files a run wrote, and how large they came out, is the export result's business
/// (`ExportResult`), so it stays out of here: those paths only mean anything on the machine that
/// exported them, and everything else about them follows from the resource rows below.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    /// Visual resource ID (GUID): the asset's identity, and the key to look it up by — the name below
    /// is not unique
    pub id: String,
    pub name: String,
    /// Mod providing this asset; absent when it comes from the game (see `domain::source::ModSources`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub path: String,
    /// The asset's materials, in its own order. GUID plus name (and the template they derive from),
    /// each carrying the textures and virtual textures it binds — the manifest states the asset's
    /// resources there and nowhere else, so this is the list to read them from
    pub materials: Vec<ExportMaterial>,
    pub exported_at_unix: u64,
    pub maclarian_version: String,
}

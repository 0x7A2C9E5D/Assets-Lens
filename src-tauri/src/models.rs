use std::collections::HashMap;

use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use serde::{Deserialize, Serialize};

use crate::state::MaterialInfo;

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    pub percent: f32,
}

/// Texture reference (DDS).
///
/// A row of the asset's texture list (the detail panel), and — copied, without `material_names` — the
/// entry the export manifest material binding it carries: the resource stated in full (name, path,
/// size, parameter) rather than a GUID to look up. Hence `Clone`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
    /// Names of this asset's materials that bind this texture, in the asset's own material order —
    /// what the detail panel inverts to list a material's textures. Left out of the JSON while empty,
    /// which is what a material the cache does not know yields. A material row of the export manifest
    /// leaves it out as well: there the material already contains its textures, so the relation would
    /// only be repeated the wrong way round.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub material_names: Vec<String>,
}

impl From<&TextureRef> for TextureSummary {
    fn from(value: &TextureRef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.dds_path.clone(),
            width: value.width,
            height: value.height,
            parameter_name: value.parameter_name.clone(),
            material_names: Vec::new(),
        }
    }
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
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
}

impl MaterialSummary {
    /// `known` is the name cache entry for this GUID; an unknown material keeps an empty name
    /// rather than dropping the row
    fn new(id: &str, known: Option<&MaterialInfo>) -> Self {
        Self {
            id: id.to_string(),
            name: known.map(|material| material.name.clone()).unwrap_or_default(),
            source_file: known
                .map(|material| material.source_file.clone())
                .unwrap_or_default(),
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
) -> Vec<MaterialSummary> {
    material_ids
        .iter()
        .map(|id| MaterialSummary::new(id, materials.get(id)))
        .collect()
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
    ) -> Self {
        let MaterialSummary {
            id,
            name,
            source_file,
        } = MaterialSummary::new(id, known);

        Self {
            id,
            name,
            source_file,
            // In the material's own order (its parameter order); an id the asset's texture rows do
            // not carry contributes nothing
            textures: known
                .map(|material| {
                    material
                        .texture_ids
                        .iter()
                        .filter_map(|id| textures.get(id.as_str()).map(|row| bound_texture(row)))
                        .collect()
                })
                .unwrap_or_default(),
            virtual_textures: known
                .map(|material| {
                    material
                        .virtual_textures
                        .iter()
                        .filter_map(|binding| {
                            virtual_textures
                                .get(binding.id.as_str())
                                .map(|row| bound_virtual_texture(row))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// The row of a texture as the entry of the material binding it
fn bound_texture(row: &TextureSummary) -> TextureSummary {
    TextureSummary {
        material_names: Vec::new(),
        ..row.clone()
    }
}

/// The row of a virtual texture as the entry of the material binding it
fn bound_virtual_texture(row: &VirtualTextureSummary) -> VirtualTextureSummary {
    VirtualTextureSummary {
        material_names: Vec::new(),
        ..row.clone()
    }
}

/// The materials of `value` in the asset's own order, as the export manifest lists them: each one
/// carrying the asset's texture and virtual texture rows for the resources it binds.
///
/// Those rows are the manifest's only statement of the asset's resources — the material containing them
/// says which material they belong to, so a separate list beside the materials would only repeat it.
pub fn manifest_materials(
    value: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
    textures: &[TextureSummary],
    virtual_textures: &[VirtualTextureSummary],
) -> Vec<ExportMaterial> {
    let textures: HashMap<&str, &TextureSummary> = textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();
    let virtual_textures: HashMap<&str, &VirtualTextureSummary> = virtual_textures
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect();

    value
        .material_ids
        .iter()
        .map(|id| {
            ExportMaterial::new(
                id,
                materials.get(id),
                &textures,
                &virtual_textures,
            )
        })
        .collect()
}

/// The entries of the material cache that `material_ids` reference, as an owned subset.
///
/// A material row of the export manifest reads the resources it binds out of these entries (see
/// `manifest_materials`), and the cache they live in is not handed to the export task — so the export
/// takes the handful of entries it needs rather than the whole cache (one entry per material of the
/// game, each holding its own texture lists).
pub fn materials_of(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> HashMap<String, MaterialInfo> {
    material_ids
        .iter()
        .filter_map(|id| materials.get(id).map(|material| (id.clone(), material.clone())))
        .collect()
}

/// Names of the materials of one asset that bind the resource `id`. More than one is possible (two
/// materials of the same mesh may share a mask), so this is a list; a material with no name
/// contributes nothing, because a bare GUID would only repeat what the material section already
/// shows.
///
/// `is_bound` decides whether a material binds the resource — a regular texture is one of its
/// `texture_ids`, a virtual texture one of its `virtual_textures`, and both are asked the same way.
fn material_names_for(
    is_bound: impl Fn(&MaterialInfo) -> bool,
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> Vec<String> {
    material_ids
        .iter()
        .filter_map(|material_id| materials.get(material_id))
        .filter(|material| is_bound(material))
        .map(|material| material.name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Parameter the asset's materials bind the virtual texture `id` with.
///
/// The name belongs to the binding rather than to the resource, so it is taken from the first
/// material that binds it — in the shipped data a virtual texture is bound once and with the same
/// name everywhere, which is what makes one name per row enough.
fn virtual_texture_parameter(
    id: &str,
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> Option<String> {
    material_ids
        .iter()
        .filter_map(|material_id| materials.get(material_id))
        .flat_map(|material| material.virtual_textures.as_slice())
        .find(|binding| binding.id == id)
        .map(|binding| binding.parameter_name.clone())
        .filter(|name| !name.is_empty())
}

/// Streaming virtual texture reference (GTex).
///
/// A row of the asset's virtual texture list (the detail panel), and — copied, without
/// `material_names` — the entry the export manifest material binding it carries, which is why it is
/// `Clone`. Its `width` / `height` are filled by whoever holds the archives, for the detail rows and
/// for the entries inside a material alike (see `export::fill_vt_sizes`).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
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
    /// Names of this asset's materials that bind this virtual texture, in the asset's own material
    /// order — what the detail panel inverts to list a material's virtual textures. Left out of the
    /// JSON while empty, which is what a material the cache does not know yields. A material row of the
    /// export manifest leaves it out as well: there the material already contains its virtual textures,
    /// so the relation would only be repeated the wrong way round.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub material_names: Vec<String>,
    /// Parameter the binding fills (e.g. `virtualtexture`), read off the asset's materials — it
    /// belongs to the binding, not to the resource. Absent from the JSON while unset: the names come
    /// from the materials' templates (see `state::fill_virtual_texture_parameters`), which a detail view
    /// and an export both read up front (`commands::ensure_virtual_texture_parameters`)
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
            hash: value.gtex_hash.clone(),
            path: matched.map(|m| m.gtp_path.clone()).unwrap_or_default(),
            // Settled later: reading the size needs the archives, which this constructor is not given
            width: None,
            height: None,
            // Settled by the caller, which is the only place holding the material cache
            material_names: Vec::new(),
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

/// The textures of `value`, each carrying the names of the asset's materials that bind it.
///
/// The texture list of the detail panel, and the source the export manifest builds its material rows
/// from (see `manifest_materials`): a texture means the same thing in both, the resource plus the
/// materials it belongs to.
pub fn texture_summaries(
    value: &VisualAsset,
    materials: &HashMap<String, MaterialInfo>,
) -> Vec<TextureSummary> {
    value
        .textures
        .iter()
        .map(|texture| {
            let mut summary = TextureSummary::from(texture);
            summary.material_names = material_names_for(
                |material| material.texture_ids.iter().any(|id| id == &texture.id),
                &value.material_ids,
                materials,
            );
            summary
        })
        .collect()
}

/// The virtual textures of `value`, each carrying the names of the asset's materials that bind it and
/// the parameter the binding fills.
///
/// The virtual texture list of the detail panel, and the source the export manifest builds its material
/// rows from (see `manifest_materials`). The parameter name is only ever filled once the caller
/// resolved it — the templates carrying it are read by a detail view and by an export
/// (`commands::ensure_virtual_texture_parameters`), not by this function — so a row stays without one
/// until then.
pub fn virtual_texture_summaries(
    value: &VisualAsset,
    matches: &[GtpMatch],
    materials: &HashMap<String, MaterialInfo>,
) -> Vec<VirtualTextureSummary> {
    value
        .virtual_textures
        .iter()
        .map(|vt| {
            let mut summary =
                VirtualTextureSummary::new(vt, match_for_hash(matches, &vt.gtex_hash));
            summary.material_names = material_names_for(
                |material| {
                    material
                        .virtual_textures
                        .iter()
                        .any(|binding| binding.id == vt.id)
                },
                &value.material_ids,
                materials,
            );
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
    pub path: String,
    pub materials: Vec<MaterialSummary>,
    pub textures: Vec<TextureSummary>,
    pub virtual_textures: Vec<VirtualTextureSummary>,
}

impl VisualAssetDetail {
    /// `matches` are the page files resolved for this asset's virtual textures; pass an empty
    /// slice to skip the lookup (the rows then show the hash without a page file).
    /// `materials` is the name cache built with the database: it labels the material rows and
    /// decides which material each texture row belongs to
    pub fn new(
        value: &VisualAsset,
        matches: &[GtpMatch],
        materials: &HashMap<String, MaterialInfo>,
    ) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            path: value.gr2_path.clone(),
            materials: material_summaries(&value.material_ids, materials),
            textures: texture_summaries(value, materials),
            virtual_textures: virtual_texture_summaries(value, matches, materials),
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
    pub material_count: usize,
    pub texture_count: usize,
    pub virtual_texture_count: usize,
}

impl From<&VisualAsset> for VisualSummary {
    fn from(value: &VisualAsset) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
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

/// Content of asset.json
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    /// Visual resource ID (GUID): the asset's identity, and the key to look it up by — the name below
    /// is not unique
    pub id: String,
    pub name: String,
    pub path: String,
    /// Mesh format that was exported: gr2 / glb
    pub mesh_format: MeshFormat,
    pub source: String,
    /// The asset's materials, in its own order. GUID plus name (and the template they derive from),
    /// each carrying the textures and virtual textures it binds — the manifest states the asset's
    /// resources there and nowhere else, so this is the list to read them from
    pub materials: Vec<ExportMaterial>,
    /// Exported artifacts; asset.json itself is deliberately not listed here
    pub files: Vec<ExportedFile>,
    pub exported_at_unix: u64,
    pub maclarian_version: String,
}

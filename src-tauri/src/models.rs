use std::collections::HashMap;

use maclarian::merged::{GtpMatch, TextureRef, VirtualTextureRef, VisualAsset};
use serde::{Deserialize, Serialize};
use specta::Type;
use specta_typescript::Number;

use crate::state::MaterialInfo;

/// Build progress, pushed to the frontend through a Tauri Channel
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BuildProgress {
    #[specta(type = Number)]
    pub percent: f32,
}

/// Texture reference (DDS): a detail row, and the cloned entry a material row of `asset.json` carries.
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TextureSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub parameter_name: Option<String>,
    /// Names of this asset's materials that bind this texture, in material order; omitted while empty.
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

/// Material reference: the GUID is the identity, the name makes the row readable, and `source_file`
/// is the template the material is defined in.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSummary {
    pub id: String,
    /// Human-readable name; empty when unknown, so the panel falls back to the GUID
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
}

impl MaterialSummary {
    /// An unknown material keeps an empty name rather than dropping the row
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

/// Summaries of `material_ids` in the order the asset lists them; unknown materials keep an empty
/// name instead of dropping out.
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
    /// `known` is the name cache entry; resources are looked up in the asset's own rows by GUID.
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
            // In the material's own order; ids without a texture row are dropped
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

/// The materials of `value` in asset order, each carrying the texture and virtual texture rows it
/// binds.
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

/// The material cache entries `material_ids` reference, as an owned subset.
pub fn materials_of(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> HashMap<String, MaterialInfo> {
    material_ids
        .iter()
        .filter_map(|id| materials.get(id).map(|material| (id.clone(), material.clone())))
        .collect()
}

/// Names of the asset's materials that bind `id`, in asset order; unnamed materials contribute
/// nothing. `is_bound` decides whether a material binds the resource.
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

/// Parameter the asset's materials bind the virtual texture `id` with; taken from the first binding,
/// since shipped data binds a virtual texture only once.
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

/// Streaming virtual texture reference (GTex): a detail row, and the cloned entry a material row
/// carries.
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VirtualTextureSummary {
    pub id: String,
    pub name: String,
    pub hash: String,
    /// Page file (`.gtp`) inside its archive; empty when no lookup was run or nothing matched
    pub path: String,
    /// Pixel size read out of the page file's GTS; `None` when the hash did not resolve or the GTS
    /// could not be read.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Names of this asset's materials that bind this virtual texture, in material order; omitted
    /// while empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub material_names: Vec<String>,
    /// Parameter the binding fills (e.g. `virtualtexture`); omitted while unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_name: Option<String>,
}

impl VirtualTextureSummary {
    /// `matched` is the page file resolved for this hash, if any.
    pub fn new(value: &VirtualTextureRef, matched: Option<&GtpMatch>) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            hash: value.gtex_hash.clone(),
            path: matched.map(|m| m.gtp_path.clone()).unwrap_or_default(),
            // Filled once the archives are available
            width: None,
            height: None,
            // Filled by the caller, which holds the material cache
            material_names: Vec::new(),
            parameter_name: None,
        }
    }

    /// A failed lookup leaves both fields unset: a size must not fail a detail view.
    pub fn set_size(&mut self, size: Option<(u32, u32)>) {
        if let Some((width, height)) = size {
            self.width = Some(width);
            self.height = Some(height);
        }
    }
}

/// The page file resolved for one GTex hash — a plain lookup, as `GtpMatch::gtex_hash` echoes it.
pub fn match_for_hash<'a>(matches: &'a [GtpMatch], hash: &str) -> Option<&'a GtpMatch> {
    let hash = hash.trim();
    matches
        .iter()
        .find(|matched| matched.gtex_hash.eq_ignore_ascii_case(hash))
}

/// The textures of `value`, each carrying the names of the materials that bind it.
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

/// The virtual textures of `value`, each carrying the materials that bind it and its parameter name
/// (filled once the caller resolves it).
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
#[derive(Serialize, Type)]
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
    /// `matches` are the resolved page files (empty to skip the lookup); `materials` is the name
    /// cache.
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

/// 3D preview payload: the GR2 mesh converted to GLB, Base64-encoded to avoid a huge JSON byte array.
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreview {
    pub base64: String,
}

/// Visual asset list item: highlights the composition of the visual itself
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VisualSummary {
    /// Visual GUID: the row identity, since names are not unique
    pub id: String,
    pub name: String,
    #[specta(type = Number)]
    pub material_count: usize,
    #[specta(type = Number)]
    pub texture_count: usize,
    #[specta(type = Number)]
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
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStats {
    #[specta(type = Number)]
    pub visual_count: usize,
    #[specta(type = Number)]
    pub material_count: usize,
    #[specta(type = Number)]
    pub texture_count: usize,
    #[specta(type = Number)]
    pub virtual_texture_count: usize,
}

/// App metadata for the About page
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Application version (compile-time, kept in sync with tauri.conf.json)
    pub version: String,
}

/// Generic pagination result
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    #[specta(type = Number)]
    pub total: usize,
    #[specta(type = Number)]
    pub offset: usize,
}

/* ---------- Asset export ---------- */

/// Mesh export format: the raw GR2 out of the archive, or a converted GLB
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
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

/// Texture export format, applied to regular and virtual textures.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Type)]
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
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// Mesh output format: raw GR2 or converted GLB
    #[serde(default)]
    pub mesh_format: MeshFormat,
    /// Texture output format, covering regular and virtual textures
    #[serde(default)]
    pub texture_format: TextureFormat,
}

/// Export progress, pushed through a Channel
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    /// Phase: prepare / model / textures / virtualTextures / manifest / done
    pub phase: String,
    pub current_file: Option<String>,
    #[specta(type = Number)]
    pub percent: f32,
}

/// One exported artifact
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    /// Artifact kind: gr2 / glb / dds / png
    pub kind: String,
    #[specta(type = Number)]
    pub size_bytes: usize,
}

/// Export warning: `code` is a stable key mapped to i18n copy, `detail` the raw path or error.
#[derive(Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub code: String,
    pub detail: String,
}

/// Export result
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// The directory actually written to (destDir/<asset name>)
    pub output_dir: String,
    pub files: Vec<ExportedFile>,
    /// Non-fatal issues: missing textures, failed PNG conversions, etc.
    pub warnings: Vec<ExportWarning>,
}

/// Content of `asset.json`: the asset and its resources, not a listing of the export directory (that
/// is `ExportResult`).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportManifest {
    /// Visual GUID, the lookup key; names are not unique
    pub id: String,
    pub name: String,
    pub path: String,
    /// The asset's materials in order, each carrying the resources it binds.
    pub materials: Vec<ExportMaterial>,
    pub exported_at_unix: u64,
    pub maclarian_version: String,
}

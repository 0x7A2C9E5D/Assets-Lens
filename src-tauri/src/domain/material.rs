//! What a material is, which resources it binds, and the rows the app lists it as — the material rows
//! of the detail panel and the entries of the export manifest.

use std::collections::HashMap;

use maclarian::merged::VisualAsset;
use serde::{Deserialize, Serialize};

use crate::domain::source::{source_of, ModSources};
use crate::domain::texture::TextureSummary;
use crate::domain::virtual_textures::VirtualTextureSummary;

/// One material of the built database: the name that makes its GUID readable, plus the resources it
/// binds (GUIDs, in parameter order).
///
/// `Clone` so a command can hand the entries of one asset to an export task without copying the whole
/// cache (see `materials_of`). Serialized because the whole map is persisted to disk with the
/// database it came out of (`crate::infrastructure::cache`), which is what saves a build on the
/// next launch: the names are not in the database itself (see `crate::state::extract_materials`),
/// so they have to travel with it.
#[derive(Clone, Serialize, Deserialize)]
pub struct MaterialInfo {
    /// Human-readable name from `MaterialBank` (e.g. `BEAR_Body_A`); empty when the resource has none
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// GUIDs of the textures this material binds
    pub texture_ids: Vec<String>,
    /// The virtual textures this material binds. An asset's virtual texture list is the union over
    /// its materials, so this is what tells those rows which material they came from: a virtual
    /// texture is only ever reachable through the material that parameterizes it.
    pub virtual_textures: Vec<VirtualTextureBinding>,
}

/// A virtual texture one material binds, together with the parameter that binding fills.
#[derive(Clone, Serialize, Deserialize)]
pub struct VirtualTextureBinding {
    /// GUID of the virtual texture resource (`VirtualTextureBank`)
    pub id: String,
    /// `ParameterName` of the binding node (e.g. `virtualtexture`, `overlayvirtualtexture`). Empty
    /// until a detail view resolves it: maclarian keeps only the GUID of a binding
    /// (see `virtual_texture_params`), so the name is read off the material's template on demand
    /// (`commands::ensure_virtual_texture_parameters`) and stays empty only until then.
    pub parameter_name: String,
}

/// Fill in the parameter name of every virtual texture binding from the material's own template.
///
/// The names are read separately from the database — maclarian drops them, and reading them is a
/// per-detail-view job (see `virtual_texture_params`) — so the two are joined here. Bindings and
/// names are paired by position, which holds because both come out in document order. A material the
/// reader did not reach keeps its empty names, and the panel then renders the chip without one.
pub fn fill_virtual_texture_parameters(
    materials: &mut HashMap<String, MaterialInfo>,
    parameters: &HashMap<String, Vec<String>>,
) {
    for (material_id, names) in parameters {
        let Some(material) = materials.get_mut(material_id) else {
            continue;
        };

        for (binding, name) in material.virtual_textures.iter_mut().zip(names) {
            if binding.parameter_name.is_empty() {
                binding.parameter_name.clone_from(name);
            }
        }
    }
}

/// The entries of the material cache that `material_ids` reference, as an owned subset.
///
/// A material row of the export manifest reads the resources it binds out of these entries (see
/// `manifest_materials`), and the cache they live in is not handed to the export task
/// — so the export takes the handful of entries it needs rather than the whole cache (one entry per
/// material of the game, each holding its own texture lists).
pub fn materials_of(
    material_ids: &[String],
    materials: &HashMap<String, MaterialInfo>,
) -> HashMap<String, MaterialInfo> {
    material_ids
        .iter()
        .filter_map(|id| {
            materials
                .get(id)
                .map(|material| (id.clone(), material.clone()))
        })
        .collect()
}

/// Parameter the asset's materials bind the virtual texture `id` with.
///
/// The name belongs to the binding rather than to the resource, so it is taken from the first
/// material that binds it — in the shipped data a virtual texture is bound once and with the same
/// name everywhere, which is what makes one name per row enough.
pub(crate) fn virtual_texture_parameter(
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
pub(crate) fn fill_material_bindings(
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
/// resources of a material from one place instead of joining two lists by GUID. What a row of the
/// detail panel states the other way round (its `bindings`) is left out here: a material containing
/// its textures says that already.
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

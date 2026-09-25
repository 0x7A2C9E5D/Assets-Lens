//! Folding a mod's resources into the built database and the material cache, by GUID.

use std::collections::HashMap;
use std::path::Path;

use maclarian::merged::{MergedDatabase, TextureRef, VirtualTextureRef, VisualAsset};

use crate::domain::material::{MaterialInfo, VirtualTextureBinding};
use crate::domain::source::ModSources;

use super::{ModAssets, ModMaterial, TextureParam};

/// Fold one mod's resources into the built database and the material cache.
///
/// The order is what makes the merge work: the mod's own textures and virtual textures are inserted
/// first, so the references resolved below can reach them; then each visual is resolved against the
/// merged view — a material the mod ships, or one the game already had, because a mod visual
/// routinely binds the game's own materials and those only exist in the cache.
///
/// A resource the mod redefines overrides the game's by GUID, and every GUID the mod provides is
/// recorded in `sources`, which is what the UI and `asset.json` label it with. Everything the game
/// provides stays absent from `sources`, i.e. reads as the base game.
pub fn merge_into(
    db: &mut MergedDatabase,
    assets: ModAssets,
    materials: &mut HashMap<String, MaterialInfo>,
    sources: &mut ModSources,
) {
    insert_textures(db, sources, &assets.name, assets.textures);
    insert_virtual_textures(db, sources, &assets.name, assets.virtual_textures);
    insert_visuals(db, sources, &assets.name, assets.visuals, &assets.materials, materials);
    insert_materials(materials, sources, &assets.name, assets.materials);
}

/// Insert the mod's own textures by GUID, recording each one as provided by `name`
fn insert_textures(
    db: &mut MergedDatabase,
    sources: &mut ModSources,
    name: &str,
    textures: HashMap<String, TextureRef>,
) {
    for (id, texture) in textures {
        sources.insert(id.clone(), name.to_string());
        db.textures.insert(id, texture);
    }
}

/// Insert the mod's own virtual textures by GUID, recording each one as provided by `name`
fn insert_virtual_textures(
    db: &mut MergedDatabase,
    sources: &mut ModSources,
    name: &str,
    virtual_textures: HashMap<String, VirtualTextureRef>,
) {
    for (id, virtual_texture) in virtual_textures {
        sources.insert(id.clone(), name.to_string());
        db.virtual_textures.insert(id, virtual_texture);
    }
}

/// Insert the mod's visuals, resolving the references of each one against the merged view first: a
/// material the mod ships, or one the game already had
fn insert_visuals(
    db: &mut MergedDatabase,
    sources: &mut ModSources,
    name: &str,
    visuals: Vec<VisualAsset>,
    mod_materials: &HashMap<String, ModMaterial>,
    materials: &HashMap<String, MaterialInfo>,
) {
    for mut visual in visuals {
        resolve(&mut visual, mod_materials, materials, db);
        sources.insert(visual.id.clone(), name.to_string());
        insert_visual(db, visual);
    }
}

/// Index one resolved visual: by GR2 file name (appended rather than replaced, one GR2 can back
/// several visuals), by name, and by id
fn insert_visual(db: &mut MergedDatabase, visual: VisualAsset) {
    let gr2_name = gr2_file_name(&visual);
    if !gr2_name.is_empty() {
        db.visuals_by_gr2
            .entry(gr2_name)
            .or_default()
            .push(visual.id.clone());
    }
    db.visuals_by_name.insert(visual.name.clone(), visual.id.clone());
    db.visuals_by_id.insert(visual.id.clone(), visual);
}

/// File name of a visual's GR2 path; empty when the path names no file
fn gr2_file_name(visual: &VisualAsset) -> String {
    Path::new(&visual.gr2_path)
        .file_name()
        .map(|file| file.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Fold the mod's own materials into the cache, recording each one as provided by `name`
fn insert_materials(
    materials: &mut HashMap<String, MaterialInfo>,
    sources: &mut ModSources,
    name: &str,
    mod_materials: HashMap<String, ModMaterial>,
) {
    for (id, material) in mod_materials {
        sources.insert(id.clone(), name.to_string());
        materials.insert(id, material_info(material));
    }
}

/// The cache entry of one mod material
fn material_info(material: ModMaterial) -> MaterialInfo {
    MaterialInfo {
        name: material.name,
        source_file: material.source_file,
        texture_ids: texture_ids_of(material.textures),
        virtual_textures: bindings_of(material.virtual_textures),
    }
}

/// The texture GUIDs of one material's texture parameters, in parameter order
fn texture_ids_of(textures: Vec<TextureParam>) -> Vec<String> {
    textures.into_iter().map(|param| param.texture_id).collect()
}

/// The virtual texture bindings of one material. Names stay empty, exactly as they do for the game's
/// own materials read out of the database: the parameter of a binding belongs to the material's
/// template and is read per detail view (see `domain::material::fill_virtual_texture_parameters`)
fn bindings_of(ids: Vec<String>) -> Vec<VirtualTextureBinding> {
    ids.into_iter()
        .map(|id| VirtualTextureBinding {
            id,
            parameter_name: String::new(),
        })
        .collect()
}

/// Resolve the texture and virtual texture references of one mod visual, in the order maclarian
/// resolves them: through each material the visual names, keeping the first binding of a resource and
/// dropping the ones that name a resource the index does not hold.
///
/// A material the mod ships brings the parameter name of every binding. A material the game already
/// had does not: the cache keeps GUIDs only (see `application::state::extract_materials`), so those
/// texture rows come out without a parameter — they are still listed under their material.
fn resolve(
    visual: &mut VisualAsset,
    mod_materials: &HashMap<String, ModMaterial>,
    materials: &HashMap<String, MaterialInfo>,
    db: &MergedDatabase,
) {
    let mut rows = ResolvedRows::default();
    for material_id in &visual.material_ids {
        rows.push_material(db, material_id, mod_materials, materials);
    }
    visual.textures = rows.textures;
    visual.virtual_textures = rows.virtual_textures;
}

/// The resource rows one visual resolves to
#[derive(Default)]
struct ResolvedRows {
    textures: Vec<TextureRef>,
    virtual_textures: Vec<VirtualTextureRef>,
}

impl ResolvedRows {
    /// Resolve one material of the visual: from the mod when it ships it, from the cache otherwise.
    /// A material neither of them knows contributes nothing.
    fn push_material(
        &mut self,
        db: &MergedDatabase,
        material_id: &str,
        mod_materials: &HashMap<String, ModMaterial>,
        materials: &HashMap<String, MaterialInfo>,
    ) {
        if let Some(material) = mod_materials.get(material_id) {
            self.push_mod_material(db, material);
        } else if let Some(material) = materials.get(material_id) {
            self.push_cached_material(db, material);
        }
    }

    /// The rows of a material the mod ships, each carrying the parameter name of its binding
    fn push_mod_material(&mut self, db: &MergedDatabase, material: &ModMaterial) {
        for param in &material.textures {
            push_texture(&mut self.textures, &db.textures, &param.texture_id, &param.parameter_name);
        }
        for id in &material.virtual_textures {
            push_virtual_texture(&mut self.virtual_textures, &db.virtual_textures, id);
        }
    }

    /// The rows of a material the game already had: the cache keeps GUIDs only, so its texture rows
    /// come out without a parameter (they are still listed under their material)
    fn push_cached_material(&mut self, db: &MergedDatabase, material: &MaterialInfo) {
        for texture_id in &material.texture_ids {
            push_texture(&mut self.textures, &db.textures, texture_id, "");
        }
        for binding in &material.virtual_textures {
            push_virtual_texture(&mut self.virtual_textures, &db.virtual_textures, &binding.id);
        }
    }
}

/// Add one bound texture to a visual's rows, unless a material already contributed it (two materials
/// of one mesh may share a texture)
fn push_texture(
    rows: &mut Vec<TextureRef>,
    textures: &HashMap<String, TextureRef>,
    id: &str,
    parameter_name: &str,
) {
    let Some(texture) = textures.get(id) else {
        return;
    };
    if rows.iter().any(|row| row.id == texture.id) {
        return;
    }

    let mut row = texture.clone();
    // An empty parameter name leaves the row without one, the same state an unresolved binding is in
    row.parameter_name = (!parameter_name.is_empty()).then(|| parameter_name.to_string());
    rows.push(row);
}

/// Add one bound virtual texture to a visual's rows, unless a material already contributed it
fn push_virtual_texture(
    rows: &mut Vec<VirtualTextureRef>,
    virtual_textures: &HashMap<String, VirtualTextureRef>,
    id: &str,
) {
    let Some(virtual_texture) = virtual_textures.get(id) else {
        return;
    };
    if rows.iter().any(|row| row.id == virtual_texture.id) {
        return;
    }

    rows.push(virtual_texture.clone());
}

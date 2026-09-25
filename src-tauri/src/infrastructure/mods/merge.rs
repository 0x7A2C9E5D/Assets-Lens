//! Folding a mod's resources into the built database and the material cache, by GUID.

use std::collections::HashMap;
use std::path::Path;

use maclarian::merged::{MergedDatabase, TextureRef, VirtualTextureRef, VisualAsset};

use crate::domain::material::{MaterialInfo, VirtualTextureBinding};
use crate::domain::source::ModSources;

use super::{ModAssets, ModMaterial};

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
    let ModAssets {
        name,
        visuals,
        materials: mod_materials,
        textures,
        virtual_textures,
    } = assets;

    for (id, texture) in textures {
        sources.insert(id.clone(), name.clone());
        db.textures.insert(id, texture);
    }
    for (id, virtual_texture) in virtual_textures {
        sources.insert(id.clone(), name.clone());
        db.virtual_textures.insert(id, virtual_texture);
    }

    for mut visual in visuals {
        resolve(&mut visual, &mod_materials, materials, db);
        sources.insert(visual.id.clone(), name.clone());
        // Keyed by file name, and appended rather than replaced: one GR2 can back several visuals
        let gr2_name = Path::new(&visual.gr2_path)
            .file_name()
            .map(|file| file.to_string_lossy().to_string())
            .unwrap_or_default();
        if !gr2_name.is_empty() {
            db.visuals_by_gr2
                .entry(gr2_name)
                .or_default()
                .push(visual.id.clone());
        }
        db.visuals_by_name
            .insert(visual.name.clone(), visual.id.clone());
        db.visuals_by_id.insert(visual.id.clone(), visual);
    }

    for (id, material) in mod_materials {
        sources.insert(id.clone(), name.clone());
        materials.insert(
            id,
            MaterialInfo {
                name: material.name,
                source_file: material.source_file,
                texture_ids: material
                    .textures
                    .into_iter()
                    .map(|param| param.texture_id)
                    .collect(),
                // Names stay empty, exactly as they do for the game's own materials read out of the
                // database: the parameter of a binding belongs to the material's template and is read
                // per detail view (see `domain::material::fill_virtual_texture_parameters`)
                virtual_textures: material
                    .virtual_textures
                    .into_iter()
                    .map(|id| VirtualTextureBinding {
                        id,
                        parameter_name: String::new(),
                    })
                    .collect(),
            },
        );
    }
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
    let mut resolved_textures: Vec<TextureRef> = Vec::new();
    let mut resolved_virtual_textures: Vec<VirtualTextureRef> = Vec::new();

    for material_id in &visual.material_ids {
        if let Some(material) = mod_materials.get(material_id) {
            for param in &material.textures {
                push_texture(
                    &mut resolved_textures,
                    &db.textures,
                    &param.texture_id,
                    &param.parameter_name,
                );
            }
            for id in &material.virtual_textures {
                push_virtual_texture(&mut resolved_virtual_textures, &db.virtual_textures, id);
            }
        } else if let Some(material) = materials.get(material_id) {
            for texture_id in &material.texture_ids {
                push_texture(&mut resolved_textures, &db.textures, texture_id, "");
            }
            for binding in &material.virtual_textures {
                push_virtual_texture(
                    &mut resolved_virtual_textures,
                    &db.virtual_textures,
                    &binding.id,
                );
            }
        }
    }

    visual.textures = resolved_textures;
    visual.virtual_textures = resolved_virtual_textures;
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

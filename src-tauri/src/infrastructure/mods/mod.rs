//! Mod support: read the banks a mod ships and fold them into the built database.
//!
//! A mod ships its banks either as one file per resource or as a single `_merged.lsf`, and maclarian
//! only ever walks a merged file, so its parser never sees the per-resource ones. Both are read here
//! instead. The format is the one maclarian parses — `VisualBank` / `MaterialBank` / `TextureBank` /
//! `VirtualTextureBank` regions, each holding `Resource` nodes — so every field reading below mirrors
//! `maclarian::merged::parser`.
//!
//! Mod resources land in the same maps as the game's own and override them by GUID: the game data is
//! parsed first, and a mod is merged over it.
//!
//! Two things a mod contributes are deliberately left out. Its `CharacterVisualBank` and
//! `MaterialPresetBank` files are presets that point at visuals and materials the game already has,
//! so indexing them would only add rows with no mesh and no material of their own. And a mod's virtual
//! textures are indexed but not extracted: the page file lookup searches the game's own archive (see
//! `application::state::vt_pak`), so those rows stay without a path and a size.

use std::collections::HashMap;

use maclarian::formats::lsx::LsxRegion;
use maclarian::merged::{TextureRef, VirtualTextureRef, VisualAsset};

use self::read::{attr, resource_nodes};

mod merge;
mod naming;
mod read;

pub use merge::merge_into;
pub use read::read_mod;

/// A texture a mod material binds, read from its `MaterialBank`.
///
/// The parameter name stays paired with the texture GUID here: the name is what labels the texture row
/// of a visual, while `MaterialInfo` — the shape the material cache uses — keeps GUIDs only.
struct TextureParam {
    parameter_name: String,
    texture_id: String,
}

/// One material read from a mod's `MaterialBank`
struct ModMaterial {
    name: String,
    source_file: String,
    /// The textures it binds, in parameter order
    textures: Vec<TextureParam>,
    /// GUIDs of the virtual textures it binds
    virtual_textures: Vec<String>,
}

/// Everything one mod contributes to the index
pub struct ModAssets {
    /// The mod's name, as the UI labels its resources
    name: String,
    /// Visuals whose references are still unresolved (`textures` / `virtual_textures` are empty):
    /// resolving needs the whole database, which only `merge_into` has
    visuals: Vec<VisualAsset>,
    materials: HashMap<String, ModMaterial>,
    textures: HashMap<String, TextureRef>,
    virtual_textures: HashMap<String, VirtualTextureRef>,
}

impl ModAssets {
    pub(super) fn new(name: String) -> Self {
        Self {
            name,
            visuals: Vec::new(),
            materials: HashMap::new(),
            textures: HashMap::new(),
            virtual_textures: HashMap::new(),
        }
    }

    /// Add the visuals of one `VisualBank` region
    pub(super) fn read_visuals(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "VisualBank") {
            let id = attr(resource, "ID");
            let gr2_path = attr(resource, "SourceFile");
            // Without either of these the visual could neither be shown nor previewed, which is where
            // maclarian draws the same line
            if id.is_empty() || gr2_path.is_empty() {
                continue;
            }

            let mut material_ids: Vec<String> = Vec::new();
            for child in resource.children.iter().filter(|c| c.id == "Objects") {
                let material_id = attr(child, "MaterialID");
                if !material_id.is_empty() && !material_ids.contains(&material_id) {
                    material_ids.push(material_id);
                }
            }

            self.visuals.push(VisualAsset {
                id,
                name: attr(resource, "Name"),
                gr2_path,
                source_pak: String::new(),
                material_ids,
                textures: Vec::new(),
                virtual_textures: Vec::new(),
            });
        }
    }

    /// Add the materials of one `MaterialBank` region
    pub(super) fn read_materials(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "MaterialBank") {
            let id = attr(resource, "ID");
            if id.is_empty() {
                continue;
            }

            let mut textures = Vec::new();
            let mut virtual_textures: Vec<String> = Vec::new();
            for child in &resource.children {
                if child.id == "Texture2DParameters" {
                    let texture_id = attr(child, "ID");
                    if !texture_id.is_empty() {
                        textures.push(TextureParam {
                            parameter_name: attr(child, "ParameterName"),
                            texture_id,
                        });
                    }
                } else if child.id == "VirtualTextureParameters" {
                    let virtual_texture_id = attr(child, "ID");
                    if !virtual_texture_id.is_empty()
                        && !virtual_textures.contains(&virtual_texture_id)
                    {
                        virtual_textures.push(virtual_texture_id);
                    }
                }
            }

            self.materials.insert(
                id,
                ModMaterial {
                    name: attr(resource, "Name"),
                    source_file: attr(resource, "SourceFile"),
                    textures,
                    virtual_textures,
                },
            );
        }
    }

    /// Add the textures of one `TextureBank` region
    pub(super) fn read_textures(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "TextureBank") {
            let id = attr(resource, "ID");
            if id.is_empty() {
                continue;
            }

            self.textures.insert(
                id.clone(),
                TextureRef {
                    id,
                    name: attr(resource, "Name"),
                    dds_path: attr(resource, "SourceFile"),
                    source_pak: String::new(),
                    // A size the bank does not state stays 0, as it does for the game's own banks
                    width: attr(resource, "Width").parse().unwrap_or(0),
                    height: attr(resource, "Height").parse().unwrap_or(0),
                    // Filled per visual, from the parameter of the material binding it
                    parameter_name: None,
                },
            );
        }
    }

    /// Add the virtual textures of one `VirtualTextureBank` region
    pub(super) fn read_virtual_textures(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "VirtualTextureBank") {
            let id = attr(resource, "ID");
            if id.is_empty() {
                continue;
            }

            self.virtual_textures.insert(
                id.clone(),
                VirtualTextureRef {
                    id,
                    name: attr(resource, "Name"),
                    gtex_hash: attr(resource, "GTexFileName"),
                },
            );
        }
    }
}

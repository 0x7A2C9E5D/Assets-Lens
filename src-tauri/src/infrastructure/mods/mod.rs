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

use maclarian::formats::lsx::{LsxNode, LsxRegion};
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
            if let Some(visual) = visual_of(resource) {
                self.visuals.push(visual);
            }
        }
    }

    /// Add the materials of one `MaterialBank` region
    pub(super) fn read_materials(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "MaterialBank") {
            let id = attr(resource, "ID");
            if !id.is_empty() {
                self.materials.insert(id, mod_material(resource));
            }
        }
    }

    /// Add the textures of one `TextureBank` region
    pub(super) fn read_textures(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "TextureBank") {
            let id = attr(resource, "ID");
            if !id.is_empty() {
                self.textures.insert(id.clone(), mod_texture(resource, id));
            }
        }
    }

    /// Add the virtual textures of one `VirtualTextureBank` region
    pub(super) fn read_virtual_textures(&mut self, region: &LsxRegion) {
        for resource in resource_nodes(region, "VirtualTextureBank") {
            let id = attr(resource, "ID");
            if !id.is_empty() {
                self.virtual_textures.insert(id.clone(), mod_virtual_texture(resource, id));
            }
        }
    }
}

/// One visual of a `VisualBank` region, or `None` when it names no id or no mesh source: without
/// either of these the visual could neither be shown nor previewed, which is where maclarian draws
/// the same line
fn visual_of(resource: &LsxNode) -> Option<VisualAsset> {
    let id = attr(resource, "ID");
    let gr2_path = attr(resource, "SourceFile");
    if id.is_empty() || gr2_path.is_empty() {
        return None;
    }
    Some(new_visual(resource, id, gr2_path))
}

/// The visual row of a `Resource` node whose id and mesh source are already known to be set
fn new_visual(resource: &LsxNode, id: String, gr2_path: String) -> VisualAsset {
    VisualAsset {
        id,
        name: attr(resource, "Name"),
        gr2_path,
        source_pak: String::new(),
        material_ids: material_ids_of(resource),
        textures: Vec::new(),
        virtual_textures: Vec::new(),
    }
}

/// The material GUIDs a visual binds, in the order it lists them and without repeats
fn material_ids_of(resource: &LsxNode) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for child in resource.children.iter().filter(|child| child.id == "Objects") {
        let material_id = attr(child, "MaterialID");
        if !material_id.is_empty() && !ids.contains(&material_id) {
            ids.push(material_id);
        }
    }
    ids
}

/// One material of a `MaterialBank` region, with the resources it binds
fn mod_material(resource: &LsxNode) -> ModMaterial {
    let mut bound = BoundResources::default();
    for child in &resource.children {
        bound.push(child);
    }
    ModMaterial {
        name: attr(resource, "Name"),
        source_file: attr(resource, "SourceFile"),
        textures: bound.textures,
        virtual_textures: bound.virtual_textures,
    }
}

/// The resources one material binds, accumulated over the child nodes of its `Resource`
#[derive(Default)]
struct BoundResources {
    textures: Vec<TextureParam>,
    virtual_textures: Vec<String>,
}

impl BoundResources {
    /// Read one child node of a material: a texture parameter, a virtual texture parameter, or
    /// neither — every other child carries no indexed resource
    fn push(&mut self, child: &LsxNode) {
        if child.id == "Texture2DParameters" {
            self.push_texture(child);
        } else if child.id == "VirtualTextureParameters" {
            self.push_virtual_texture(child);
        }
    }

    /// One `Texture2DParameters` node: a texture GUID paired with the parameter that binds it
    fn push_texture(&mut self, child: &LsxNode) {
        let texture_id = attr(child, "ID");
        if texture_id.is_empty() {
            return;
        }
        self.textures.push(TextureParam {
            parameter_name: attr(child, "ParameterName"),
            texture_id,
        });
    }

    /// One `VirtualTextureParameters` node: a virtual texture GUID, without repeats
    fn push_virtual_texture(&mut self, child: &LsxNode) {
        let id = attr(child, "ID");
        if !id.is_empty() && !self.virtual_textures.contains(&id) {
            self.virtual_textures.push(id);
        }
    }
}

/// One texture of a `TextureBank` region, whose id is already known to be set
fn mod_texture(resource: &LsxNode, id: String) -> TextureRef {
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
    }
}

/// One virtual texture of a `VirtualTextureBank` region, whose id is already known to be set
fn mod_virtual_texture(resource: &LsxNode, id: String) -> VirtualTextureRef {
    VirtualTextureRef {
        id,
        name: attr(resource, "Name"),
        gtex_hash: attr(resource, "GTexFileName"),
    }
}

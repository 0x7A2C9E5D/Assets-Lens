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
//! `state::vt_pak`), so those rows stay without a path and a size.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use maclarian::converter::to_lsx;
use maclarian::formats::lsf::parse_lsf_bytes;
use maclarian::formats::lsx::{parse_lsx, LsxDocument, LsxNode, LsxRegion};
use maclarian::merged::{MergedDatabase, TextureRef, VirtualTextureRef, VisualAsset};

use crate::archives::Archives;
use crate::state::{MaterialInfo, ModSources, VirtualTextureBinding};

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
    fn new(name: String) -> Self {
        Self {
            name,
            visuals: Vec::new(),
            materials: HashMap::new(),
            textures: HashMap::new(),
            virtual_textures: HashMap::new(),
        }
    }

    /// Add the visuals of one `VisualBank` region
    fn read_visuals(&mut self, region: &LsxRegion) {
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
    fn read_materials(&mut self, region: &LsxRegion) {
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
    fn read_textures(&mut self, region: &LsxRegion) {
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
    fn read_virtual_textures(&mut self, region: &LsxRegion) {
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

/// Read one mod archive into the resources it contributes.
///
/// A file that fails to parse is skipped with a note: a mod is third-party data, and one unreadable
/// bank must not cost the whole build.
///
/// Only the module's bank folders are walked (see `bank_dir`), and they are read one directory at a
/// time so a `_merged.lsf` can stand in for the files beside it.
pub fn read_mod(archives: &mut Archives, index: usize) -> ModAssets {
    let stem = archives.mod_name(index);

    let paths = match archives.list_mod(index) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("[maclarian] mod {stem} skipped: {err}");
            return ModAssets::new(stem);
        }
    };

    let name = mod_display_name(archives, index, &paths, stem);
    let mut assets = ModAssets::new(name);

    let mut banks: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for path in &paths {
        if let Some(dir) = bank_dir(path) {
            banks.entry(dir).or_default().push(path);
        }
    }

    for mut files in banks.into_values() {
        // A directory carrying a merged file already holds every `.lsf` bank beside it, so those are
        // not parsed again. `.lsx` banks stay: the merged file accounts for the `.lsf` ones only.
        if files.iter().any(|path| path.ends_with("_merged.lsf")) {
            files.retain(|path| path.ends_with("_merged.lsf") || path.ends_with(".lsx"));
        }

        for path in files {
            let Some(lsx) = read_bank(archives, index, path) else {
                continue;
            };
            for region in &lsx.regions {
                match region.id.as_str() {
                    "VisualBank" => assets.read_visuals(region),
                    "MaterialBank" => assets.read_materials(region),
                    "TextureBank" => assets.read_textures(region),
                    "VirtualTextureBank" => assets.read_virtual_textures(region),
                    // Every other region (Templates, Tags, CharacterVisualBank, …) carries no indexed
                    // resource
                    _ => {}
                }
            }
        }
    }

    assets
}

/// The name a mod is labeled with: the `Name` its own `meta.lsx` declares, which is what the game and
/// the mod manager call it. The archive's file name is a hash-suffixed stem (`hairunlocked_e4aaf48d-…`),
/// so it is only the fallback — for a mod that ships no `meta.lsx`, or one whose file cannot be read.
fn mod_display_name(
    archives: &mut Archives,
    index: usize,
    paths: &[String],
    fallback: String,
) -> String {
    let Some(path) = paths.iter().find(|path| path.ends_with("meta.lsx")) else {
        return fallback;
    };

    read_bank(archives, index, path)
        .and_then(|document| {
            document
                .regions
                .iter()
                .flat_map(|region| region.nodes.iter())
                .find_map(module_name)
        })
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
}

/// The `Name` attribute of the first `ModuleInfo` node in the tree. The declaration sits one level
/// down (`Root > Config > ModuleInfo`), but the depth is not part of the format, so the walk simply
/// descends until it finds the node.
fn module_name(node: &LsxNode) -> Option<String> {
    if node.id == "ModuleInfo" {
        let name = attr(node, "Name");
        if !name.is_empty() {
            return Some(decode_entities(&name));
        }
    }

    node.children.iter().find_map(module_name)
}

/// XML entities decoded in a value read out of an attribute: the LSX reader hands the text over as it
/// stands in the file, and mod names do use escapes (`PixellBytes&apos; Adjustable Party Limit`),
/// which would otherwise reach the UI as markup. `&amp;` goes last so an escaped escape survives.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }

    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

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
                // per detail view (see `state::fill_virtual_texture_parameters`)
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
/// had does not: the cache keeps GUIDs only (see `state::extract_materials`), so those texture rows
/// come out without a parameter — they are still listed under their material.
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

/// The bank directory an archive entry belongs to, or `None` when the entry is not a bank file.
///
/// Two things have to hold: the path sits under `Public/` and carries a `Content` folder, and a
/// `[PAK]_<name>` folder appears in it. Neither has to be the file's own folder — the engine nests
/// banks under the asset tree they belong to, so folders may sit between `Content` and `[PAK]_`, and
/// a bank file may sit below `[PAK]_` in turn.
///
/// Both `.lsf` and `.lsx` banks are read: the editor emits either form, and both carry the same
/// regions.
///
/// Everything else a module ships supplies no indexed resource: `RootTemplates` and `Tags` hold
/// template and tag tables, `GUI` and the `Mods/<name>/` metadata live outside `Content` altogether.
/// Rejecting them by path keeps those files from being parsed only for every region in them to be
/// dropped.
fn bank_dir(path: &str) -> Option<&str> {
    let (dir, file) = path.rsplit_once('/')?;
    if !matches!(file.rsplit('.').next(), Some("lsf" | "lsx")) {
        return None;
    }

    let folders = dir.strip_prefix("public/")?;
    let is_content = |folder: &str| folder == "content";
    let is_bank = |folder: &str| folder.starts_with("[pak]_");
    let has_content = folders.split('/').any(is_content);
    let has_bank = folders.split('/').any(is_bank);

    (has_content && has_bank).then_some(dir)
}

/// Parse one bank file into an LSX document.
///
/// A `.lsx` bank is already XML and is parsed as it stands; a `.lsf` bank goes through maclarian's LSF
/// reader, which handles the document version and the per-segment compression, and then its
/// converter, which hands over the same document model. A file that fails any step is skipped,
/// leaving the mod's other banks unaffected.
fn read_bank(archives: &mut Archives, index: usize, path: &str) -> Option<LsxDocument> {
    let bytes = match archives.read_mod_file(index, path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("[maclarian] mod file skipped: {err}");
            return None;
        }
    };

    let xml = if path.ends_with(".lsx") {
        match String::from_utf8(bytes) {
            Ok(xml) => xml,
            Err(err) => {
                eprintln!("[maclarian] {path} is not readable as LSX: {err}");
                return None;
            }
        }
    } else {
        match parse_lsf_bytes(&bytes).and_then(|document| to_lsx(&document)) {
            Ok(xml) => xml,
            Err(err) => {
                eprintln!("[maclarian] {path} is not readable as LSF: {err}");
                return None;
            }
        }
    };

    match parse_lsx(&xml) {
        Ok(lsx) => Some(lsx),
        Err(err) => {
            eprintln!("[maclarian] {path} is not readable as LSX: {err}");
            None
        }
    }
}

/// The `Resource` nodes of a bank region.
///
/// A per-resource bank file nests them under a node named after the bank (`VisualBank` → `Resource`),
/// which is the shape maclarian reads out of a merged file; a region listing them directly is accepted
/// as well.
fn resource_nodes<'a>(region: &'a LsxRegion, bank: &str) -> Vec<&'a LsxNode> {
    let mut resources = Vec::new();
    for node in &region.nodes {
        if node.id == bank {
            resources.extend(node.children.iter().filter(|child| child.id == "Resource"));
        } else if node.id == "Resource" {
            resources.push(node);
        }
    }
    resources
}

/// Value of one attribute of a node; a node that does not carry it yields an empty string, which is
/// what every caller below tests against
fn attr(node: &LsxNode, id: &str) -> String {
    node.attributes
        .iter()
        .find(|attr| attr.id == id)
        .map(|attr| attr.value.clone())
        .unwrap_or_default()
}

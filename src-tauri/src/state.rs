use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, GtpMatch, MergedDatabase, MergedResolver};
use serde::Deserialize;

use crate::archives::{Archives, Pak};
use crate::virtual_textures::{PageFileSizeCache, PAGE_FILE_SIZE_CACHE_CAPACITY};

/// One material of the built database: its name and the resources it binds (GUIDs, in parameter
/// order). `Clone` so an export task can take a subset without copying the whole cache.
#[derive(Clone)]
pub struct MaterialInfo {
    /// Human-readable name from `MaterialBank` (e.g. `BEAR_Body_A`); empty when the resource has none
    pub name: String,
    /// Base material template (`.lsf`) the material is derived from
    pub source_file: String,
    /// GUIDs of the textures this material binds
    pub texture_ids: Vec<String>,
    /// The virtual textures this material binds; an asset's list is the union over its materials.
    pub virtual_textures: Vec<VirtualTextureBinding>,
}

/// A virtual texture one material binds, together with the parameter that binding fills.
#[derive(Clone)]
pub struct VirtualTextureBinding {
    /// GUID of the virtual texture resource (`VirtualTextureBank`)
    pub id: String,
    /// `ParameterName` of the binding node (e.g. `virtualtexture`); filled on demand from the
    /// material's template, empty until then.
    pub parameter_name: String,
}

/// One material as it appears in the serialized database, carrying only what the panel needs.
#[derive(Deserialize)]
struct RawMaterial {
    name: String,
    #[serde(default)]
    source_file: String,
    #[serde(default)]
    texture_ids: Vec<RawTextureParam>,
    /// Bare GUIDs, unlike `texture_ids`: maclarian drops the binding's parameter name. Absent for
    /// materials that bind no virtual texture, hence the default.
    #[serde(default)]
    virtual_texture_ids: Vec<String>,
}

/// A texture binding inside a material; its parameter name is already on the texture itself
#[derive(Deserialize)]
struct RawTextureParam {
    texture_id: String,
}

/// The `materials` map of a serialized `MergedDatabase`; every other field is skipped
#[derive(Deserialize)]
struct RawMaterials {
    #[serde(default)]
    materials: HashMap<String, RawMaterial>,
}

/// Read every material out of an already-built database, keyed by material GUID.
///
/// maclarian keeps `MaterialDef` crate-private, so serializing the database is the only door out.
pub fn extract_materials(db: &MergedDatabase) -> HashMap<String, MaterialInfo> {
    let json = match serde_json::to_string(db) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("[maclarian] material names unavailable: {err}");
            return HashMap::new();
        }
    };
    let parsed: RawMaterials = match serde_json::from_str(&json) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("[maclarian] material names unavailable: {err}");
            return HashMap::new();
        }
    };

    parsed
        .materials
        .into_iter()
        .map(|(id, material)| {
            (
                id,
                MaterialInfo {
                    name: material.name,
                    source_file: material.source_file,
                    texture_ids: material
                        .texture_ids
                        .into_iter()
                        .map(|param| param.texture_id)
                        .collect(),
                    // The serialized database holds bare GUIDs; the name is filled in later
                    virtual_textures: material
                        .virtual_texture_ids
                        .into_iter()
                        .map(|id| VirtualTextureBinding {
                            id,
                            parameter_name: String::new(),
                        })
                        .collect(),
                },
            )
        })
        .collect()
}

/// Fill in the parameter name of every virtual texture binding from the material's template.
///
/// Bindings and names pair by position because both come out in document order; an unreached
/// material keeps its empty names.
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

/// Global application state: game directory, resolver, built database, and the caches that keep
/// pagination order stable. Only the frontend persists the game directory (Web storage).
pub struct AppState {
    /// Shared so a build can keep working after the state lock is released; not `Clone`.
    pub resolver: Option<Arc<GameDataResolver>>,
    pub game_path: Option<PathBuf>,
    pub merged_db: Option<MergedDatabase>,
    /// Sorted visual GUIDs, so pagination order stays stable; keyed by id because names repeat.
    pub visual_ids: Vec<String>,
    /// The same GUIDs ordered by GUID, cached so sorting by ID picks a sequence instead of re-sorting.
    pub visual_ids_by_id: Vec<String>,
    /// Materials by GUID, filled once per build (see `extract_materials`).
    pub materials: HashMap<String, MaterialInfo>,
    /// PAK read pool shared by every command; `Mutex` because reading needs `&mut`.
    pub archives: Option<Arc<Mutex<Archives>>>,
    /// Page file sizes by GTS path; an empty list is a cached "no sizes here" answer.
    /// `moka` keeps it bounded and shares it by cloning the handle.
    pub page_file_sizes: PageFileSizeCache,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_ids: Vec::new(),
            visual_ids_by_id: Vec::new(),
            materials: HashMap::new(),
            archives: None,
            page_file_sizes: PageFileSizeCache::new(PAGE_FILE_SIZE_CACHE_CAPACITY),
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_ids.clear();
        self.visual_ids_by_id.clear();
        // The names were read out of the database that was just dropped
        self.materials.clear();
        // The archives still open belong to the previous directory
        self.archives = None;
        // Page file sizes were read from those archives
        self.page_file_sizes.invalidate_all();
    }

    /// The shared PAK pool, created on first use; callers clone the `Arc` and lock only around a read.
    pub fn pool(&mut self) -> Result<Arc<Mutex<Archives>>, String> {
        if let Some(pool) = &self.archives {
            return Ok(pool.clone());
        }

        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let pool = Arc::new(Mutex::new(Archives::new(game_path)));
        self.archives = Some(pool.clone());
        Ok(pool)
    }

    /// The archive holding the virtual texture page files; the lookup needs one to list.
    fn vt_pak(&self) -> Result<PathBuf, String> {
        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        let vt_pak = game_path.join(Pak::VirtualTextures.file_name());
        if !vt_pak.is_file() {
            return Err(format!(
                "{} not found in {}",
                Pak::VirtualTextures.file_name(),
                game_path.display()
            ));
        }
        Ok(vt_pak)
    }

    /// Resolve GTex hashes to page files through maclarian's lookup; a missing archive or failed
    /// lookup degrades to no virtual textures.
    ///
    /// `MergedResolver` owns the database, so it moves out and back in.
    pub fn vt_matches(&mut self, hashes: &[String]) -> Vec<GtpMatch> {
        let Some(db) = self.merged_db.take() else {
            return Vec::new();
        };
        let pak = self.vt_pak();
        let resolver = MergedResolver::from_database(db);

        let matches = match pak {
            Ok(pak) => {
                let hashes: Vec<&str> = hashes.iter().map(String::as_str).collect();
                resolver
                    .find_gtp_by_hashes_in_pak(&hashes, &pak)
                    .unwrap_or_else(|err| {
                        eprintln!("[maclarian] virtual texture lookup failed: {err}");
                        Vec::new()
                    })
            }
            Err(err) => {
                eprintln!("[maclarian] {err}");
                Vec::new()
            }
        };

        self.merged_db = Some(resolver.into_database());
        matches
    }
}

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use maclarian::merged::{GameDataResolver, GtpMatch, MergedDatabase, MergedResolver};
use serde::Deserialize;

use crate::archives::{Archives, Pak};
use crate::virtual_textures::PageFileSizes;

/// The resources a mod provides, by resource GUID. Everything the game itself provides is absent, so a
/// GUID missing here reads as the base game.
pub type ModSources = HashMap<String, String>;

/// The mod that provides a resource, or `None` when it comes from the game
pub fn source_of(sources: &ModSources, id: &str) -> Option<String> {
    sources.get(id).cloned()
}

/// One material of the built database: the name that makes its GUID readable, plus the resources it
/// binds (GUIDs, in parameter order).
///
/// `Clone` so a command can hand the entries of one asset to an export task without copying the whole
/// cache (see `models::materials_of`).
#[derive(Clone)]
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
#[derive(Clone)]
pub struct VirtualTextureBinding {
    /// GUID of the virtual texture resource (`VirtualTextureBank`)
    pub id: String,
    /// `ParameterName` of the binding node (e.g. `virtualtexture`, `overlayvirtualtexture`). Empty
    /// until a detail view resolves it: maclarian keeps only the GUID of a binding
    /// (see `virtual_texture_params`), so the name is read off the material's template on demand
    /// (`commands::ensure_virtual_texture_parameters`) and stays empty only until then.
    pub parameter_name: String,
}

/// One material as it appears in the serialized database, carrying only what the panel needs.
///
/// Deserialized into this shape rather than picked out of a `serde_json::Value`: the database
/// holds every visual and texture as well, and building that whole tree only to reach `materials`
/// is a memory spike worth skipping. Unknown fields are ignored, while `name` is required — a
/// renamed field stops here instead of quietly leaving every material without a label.
#[derive(Deserialize)]
struct RawMaterial {
    name: String,
    #[serde(default)]
    source_file: String,
    #[serde(default)]
    texture_ids: Vec<RawTextureParam>,
    /// Bare GUIDs, unlike `texture_ids`: maclarian parses a binding down to its `ID` and drops the
    /// parameter name that goes with it (see `virtual_texture_params`). Absent — not empty — for
    /// the materials that bind no virtual texture, hence the default.
    #[serde(default)]
    virtual_texture_ids: Vec<String>,
}

/// A texture binding inside a material; the parameter name it also carries is already on the
/// texture itself
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
/// maclarian keeps `MaterialDef` — and the map holding it — crate-private, so serializing the
/// database is the only door out. This runs once per build, where the cost disappears next to the
/// parse itself; nothing is cached on disk, so a rebuilt database pays it again.
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
                    // The parameter name is not in the serialized database either — the field below
                    // is a bare GUID, so `fill_virtual_texture_parameters` supplies the name later
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

/// Global application state: BG3 data directory, the resource resolver, the built database, and a
/// stable name cache for consistent pagination order.
///
/// The game data directory is not persisted here: the frontend remembers it (Web storage) and hands
/// it back through `set_game_path` on startup, so this state only lives for the current session. The
/// mod directory is not settable at all — it is always the game's own default location.
pub struct AppState {
    /// Shared rather than owned: building the database runs for minutes, and the build has to keep
    /// working on the resolver after the state lock has been released (see `build_database`).
    /// It is not cloned — `GameDataResolver` is not `Clone`.
    pub resolver: Option<Arc<GameDataResolver>>,
    pub game_path: Option<PathBuf>,
    pub merged_db: Option<MergedDatabase>,
    /// Sorted visual GUIDs kept after building, so pagination order stays stable
    /// (HashMap iteration order is not deterministic). Ids rather than names: a name can belong to
    /// several visuals, so keying the list by name would silently drop the duplicates.
    pub visual_ids: Vec<String>,
    /// The same GUIDs ordered by GUID: cached next to the name order so sorting the list by ID
    /// picks a sequence instead of re-sorting every id on each page request.
    pub visual_ids_by_id: Vec<String>,
    /// The same GUIDs ordered by their source (mod name), with the base game first: the mod table
    /// offers that column as a sort key, and it is resolved here for the same reason as the orders
    /// above.
    pub visual_ids_by_source: Vec<String>,
    /// Materials by GUID, filled once per build. The database itself cannot be queried for them
    /// (see `extract_materials`), so the names are read out at build time and kept here.
    pub materials: HashMap<String, MaterialInfo>,
    /// The mod providing each resource of the index, filled once per build. Only the resources a mod
    /// provides are in here (see `ModSources`), and it labels them in the list, the detail panel and
    /// `asset.json` alike.
    pub mod_sources: ModSources,
    /// PAK read pool shared by every command: opening an archive parses its whole file table, so the
    /// pool is created once per game directory instead of once per preview / export. `Mutex` because
    /// reading an archive needs `&mut` on its reader.
    pub archives: Option<Arc<Mutex<Archives>>>,
    /// Page file sizes by GTS path, filled by `virtual_textures::page_file_size`. Reading a GTS
    /// costs a full extraction while one GTS serves every page file of its tile set, so browsing a
    /// model whose virtual textures share a tile set would otherwise read the same file repeatedly.
    /// An empty list is a cached "no sizes here" answer, not a missing entry.
    pub page_file_sizes: HashMap<String, PageFileSizes>,
}

/// An empty state: no directory chosen and nothing built yet, which is what `new` returns
impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            resolver: None,
            game_path: None,
            merged_db: None,
            visual_ids: Vec::new(),
            visual_ids_by_id: Vec::new(),
            visual_ids_by_source: Vec::new(),
            materials: HashMap::new(),
            mod_sources: ModSources::new(),
            archives: None,
            page_file_sizes: HashMap::new(),
        }
    }

    /// Clear all caches after switching the game data directory
    pub fn reset_index(&mut self) {
        self.merged_db = None;
        self.visual_ids.clear();
        self.visual_ids_by_id.clear();
        self.visual_ids_by_source.clear();
        // The names were read out of the database that was just dropped
        self.materials.clear();
        // The sources belong to the database that was just dropped
        self.mod_sources.clear();
        // The archives still open belong to the previous directory
        self.archives = None;
        // Page file sizes were read from those archives
        self.page_file_sizes.clear();
    }

    /// The shared PAK pool, created on the first command that needs an archive. Callers clone the
    /// `Arc` back out and lock it only around a single read, so the state lock and the pool never
    /// have to be held at the same time.
    pub fn pool(&mut self) -> Result<Arc<Mutex<Archives>>, String> {
        if let Some(pool) = &self.archives {
            return Ok(pool.clone());
        }

        let game_path = self
            .game_path
            .as_ref()
            .ok_or_else(|| "BG3 Data directory is not set.".to_string())?;
        // Read once, here: the pool is built per game directory, and the mod list is part of what it
        // is built from. The mod directory is the game's own default location (see
        // `default_mods_path`) — it is not user-settable, so the list only changes when the user
        // adds or removes a mod file, which a rebuild picks up.
        let mod_paths = default_mods_path()
            .map(|dir| mod_paks(&dir))
            .unwrap_or_default();
        let pool = Arc::new(Mutex::new(Archives::new(game_path, mod_paths)));
        self.archives = Some(pool.clone());
        Ok(pool)
    }

    /// The archive that holds the virtual texture page files. maclarian's lookup needs one archive
    /// to list; which one that is cannot come from a page file (finding it is the lookup's job), so
    /// it is spelled out by kind.
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

    /// Resolve GTex hashes to their page files through maclarian's own lookup. Each `GtpMatch`
    /// carries the path inside the archive *and* the archive itself, so neither has to be guessed
    /// downstream — and unlike a hash-to-path index, a match can only exist for a hash that is
    /// actually present. A missing archive or a failed lookup degrades to "no virtual textures":
    /// they are optional, and must not abort a detail view or an export.
    ///
    /// `MergedResolver` owns the database it is built from, so the database moves out and back in
    /// (pointers move, nothing is copied); the state is left exactly as it was found.
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

/// Where the game installs its mods: `%LOCALAPPDATA%\Larian Studios\Baldur's Gate 3\Mods`, which is
/// the directory every mod manager for the game writes to.
///
/// This is fixed rather than configurable: it is where the game itself reads mods from, so scanning
/// anything else would index resources the game never loads. `None` on a machine that has never run
/// the game — and a `Mods` directory holding no `.pak` is just as valid — leaves the database built
/// from the game alone.
fn default_mods_path() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(base)
        .join("Larian Studios")
        .join("Baldur's Gate 3")
        .join("Mods");
    path.is_dir().then_some(path)
}

/// The mod archives of a directory, in ascending file name order.
///
/// The order is fixed rather than left to the file system, because it is what decides which mod wins
/// when two of them provide the same resource: a read walks the list backwards, so the last file name
/// takes precedence. A directory that cannot be listed contributes no mods rather than an error —
/// mods are optional, and the game's own data is read either way.
fn mod_paks(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pak"))
        })
        .collect();
    paths.sort();
    paths
}

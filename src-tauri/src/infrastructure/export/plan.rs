//! The export plan and its progress reporting: what an export will contain, decided before anything
//! is written, and the counter every step of the pipeline reports through.

use std::fs;
use std::path::{Path, PathBuf};

use maclarian::merged::{VirtualTextureRef, VisualAsset};

use super::virtual_textures::vt_targets_of;
use crate::domain::export::{ExportOptions, ExportProgress, MeshFormat};
use crate::domain::naming::sanitize_file_name;

/// Tracks export progress: counts finished items and reports the running percentage, so no step of
/// the pipeline has to thread a `done` counter around
pub(crate) struct ProgressTracker<'a> {
    total: usize,
    done: usize,
    on_progress: &'a dyn Fn(ExportProgress),
}

impl<'a> ProgressTracker<'a> {
    pub(crate) fn new(total: usize, on_progress: &'a dyn Fn(ExportProgress)) -> Self {
        Self {
            total,
            done: 0,
            on_progress,
        }
    }

    /// Count one finished item and report it
    pub(crate) fn item(&mut self, phase: &str, file: Option<String>) {
        self.done += 1;
        (self.on_progress)(ExportProgress {
            phase: phase.to_string(),
            current_file: file,
            percent: if self.total == 0 {
                1.0
            } else {
                self.done as f32 / self.total as f32
            },
        });
    }

    /// Report a phase boundary without counting an item (`prepare` / `done`); those carry a fixed
    /// percentage because they wrap the counted items instead of being one of them
    pub(crate) fn phase(&self, phase: &str, percent: f32) {
        (self.on_progress)(ExportProgress {
            phase: phase.to_string(),
            current_file: None,
            percent,
        });
    }
}

/// Everything decided before the first artifact is written: where it goes, in which form, and how
/// many items the progress bar will count. Bundling it keeps the steps below at four to six
/// parameters instead of threading every path and flag through the whole pipeline
pub(crate) struct ExportPlan<'a> {
    pub(crate) dir_name: String,
    pub(crate) out_dir: PathBuf,
    pub(crate) mesh_format: MeshFormat,
    pub(crate) export_textures: bool,
    pub(crate) convert_to_png: bool,
    pub(crate) vt_targets: Vec<&'a VirtualTextureRef>,
    pub(crate) progress_total: usize,
}

impl ExportPlan<'_> {
    /// Where the extracted layers of the virtual textures go
    pub(crate) fn vt_dir(&self) -> PathBuf {
        self.out_dir.join("virtual_textures")
    }
}

/// Decide what the export will contain and create its output directory
pub(super) fn plan_export<'a>(
    asset: &'a VisualAsset,
    dest_root: &Path,
    options: &ExportOptions,
) -> Result<ExportPlan<'a>, String> {
    // Textures are always exported separately (regular + virtual); the format is a single switch
    let export_textures = options.texture_format.is_export();

    let dir_name = sanitize_file_name(&asset.name);
    let out_dir = dest_root.join(&dir_name);
    fs::create_dir_all(&out_dir).map_err(|e| format!("Failed to create export directory: {e}"))?;

    let vt_targets = vt_targets_of(asset, export_textures);
    let texture_total = if export_textures {
        asset.textures.len()
    } else {
        0
    };
    // 1 mesh + 1 manifest, plus texture files and virtual textures
    let progress_total = 2 + texture_total + vt_targets.len();

    Ok(ExportPlan {
        dir_name,
        out_dir,
        mesh_format: options.mesh_format,
        export_textures,
        convert_to_png: options.texture_format.is_png(),
        vt_targets,
        progress_total,
    })
}

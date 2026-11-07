mod commands;
mod error;
pub mod export;
mod models;
pub mod archives;
mod state;
mod virtual_texture_params;
mod virtual_textures;

use std::sync::{Arc, Mutex};

use state::AppState;
use tauri_plugin_window_state::{Builder as WindowStateBuilder, StateFlags};
use tauri_specta::{collect_commands, Builder as SpectaBuilder};

use commands::{
    app_info, build_database, db_stats, detect_game_path, export_visual_asset, get_game_path,
    get_visual, get_visual_preview, list_visuals, set_game_path,
};

/// Typesafe command bindings for the frontend, generated from the command signatures
fn specta_builder() -> SpectaBuilder<tauri::Wry> {
    SpectaBuilder::<tauri::Wry>::new()
        // Rejected promises, as a plain `invoke` would behave: the frontend already catches errors
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .commands(collect_commands![
            app_info,
            detect_game_path,
            get_game_path,
            set_game_path,
            build_database,
            db_stats,
            list_visuals,
            get_visual,
            get_visual_preview,
            export_visual_asset,
        ])
}

/// Write `src/api/bindings.ts`, kept in the repository so the frontend builds without Rust.
// Debug builds only: release builds never call it, so it is compiled out without a warning
#[cfg(debug_assertions)]
fn export_bindings(builder: &SpectaBuilder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default(),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/api/bindings.ts"),
        )
        .expect("failed to export the TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Regenerated on every debug launch (`tauri dev`); release builds use the committed file
    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens the project's external links (GitHub / Nexus Mods) in the system browser
        .plugin(tauri_plugin_opener::init())
        // Restores and persists the window rectangle across launches; the frontend shows the
        // window once its first frame is ready (see `utils/window.ts`)
        .plugin(
            WindowStateBuilder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        // Session state only: the game directory lives in the frontend's Web storage and is handed
        // back through `set_game_path` on startup, so nothing is restored from disk here
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

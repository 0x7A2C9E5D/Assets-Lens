pub mod archives;
mod cache;
mod commands;
mod domain;
pub mod export;
mod models;
pub mod mods;
mod settings;
mod state;
mod virtual_texture_params;
mod virtual_textures;

use std::sync::{Arc, Mutex};

use state::AppState;

use commands::{
    app_info, build_database, cache_status, db_stats, detect_game_path, export_visual_asset,
    get_visual, get_visual_preview, list_visuals, restore_state, set_game_path,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens the project's external links (GitHub / Nexus Mods) in the system browser
        .plugin(tauri_plugin_opener::init())
        // The game directory is recorded by the backend itself (`settings`) and comes back with
        // `restore_state`; the index built from it is persisted separately, and comes back with it
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(tauri::generate_handler![
            app_info,
            detect_game_path,
            set_game_path,
            restore_state,
            build_database,
            db_stats,
            cache_status,
            list_visuals,
            get_visual,
            get_visual_preview,
            export_visual_asset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

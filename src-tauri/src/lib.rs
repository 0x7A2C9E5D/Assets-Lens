mod commands;
pub mod export;
mod models;
mod mods;
mod state;

use std::sync::{Arc, Mutex};

use state::AppState;

use commands::{
    app_info, build_database, db_stats, detect_game_path, export_visual_asset, get_game_path,
    get_visual, get_visual_preview, list_visuals, set_game_path,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens the project's external links (GitHub / Nexus Mods) in the system browser
        .plugin(tauri_plugin_opener::init())
        // Session state only: the game directory lives in the frontend's Web storage and is handed
        // back through `set_game_path` on startup, so nothing is restored from disk here
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(tauri::generate_handler![
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

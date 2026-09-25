mod application;
mod domain;
pub mod infrastructure;

use std::sync::{Arc, Mutex};

use application::commands::{
    app_info, build_database, cache_status, db_stats, detect_game_path, export_visual_asset,
    get_visual, get_visual_preview, list_visuals, restore_state, set_game_path,
};
use application::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Opens external links (GitHub / Nexus Mods) in the system browser
        .plugin(tauri_plugin_opener::init())
        // Empty on purpose: the game directory and the persisted index come back through `restore_state`
        .manage(Arc::new(Mutex::new(AppState::new())))
        .invoke_handler(handlers())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Every command the frontend can invoke
fn handlers() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool {
    tauri::generate_handler![
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
    ]
}

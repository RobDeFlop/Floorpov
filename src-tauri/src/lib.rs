mod capture;
mod recording;
mod settings;
mod combat_log;
mod hotkey;

use std::sync::Arc;
use tokio::sync::RwLock;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let capture_state = Arc::new(RwLock::new(capture::CaptureState::new()));
    let recording_state = Arc::new(RwLock::new(recording::RecordingState::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(capture_state)
        .manage(recording_state)
        .invoke_handler(tauri::generate_handler![
            greet,
            capture::start_preview,
            capture::stop_preview,
            capture::list_windows,
            recording::start_recording,
            recording::stop_recording,
            settings::get_default_output_folder,
            settings::get_folder_size,
            settings::get_recordings_list,
            settings::cleanup_old_recordings,
            combat_log::start_combat_watch,
            combat_log::stop_combat_watch,
            combat_log::emit_manual_marker,
            hotkey::register_marker_hotkey,
            hotkey::unregister_marker_hotkey,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


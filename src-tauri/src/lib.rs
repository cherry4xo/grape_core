// Prevents additional console window on Windows in release
#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_peer_id,
            commands::get_contacts,
            commands::add_contact,
            commands::remove_contact,
            commands::get_messages,
            commands::send_message,
            commands::dial_peer,
            commands::search_messages,
            commands::subscribe_channel,
            commands::unsubscribe_channel,
            commands::publish_to_channel,
            commands::list_channels,
            commands::get_dht_stats,
            commands::trigger_bootstrap,
        ])
        .setup(|app| {
            commands::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

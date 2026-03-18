// Prevents additional console window on Windows in release
#[cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_peer_id,
            commands::get_contacts,
            commands::get_chat_peers,
            commands::add_contact,
            commands::remove_contact,
            commands::rename_contact,
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
            commands::find_peer,
            commands::auth_has_seed,
            commands::auth_is_encrypted,
            commands::auth_generate_mnemonic,
            commands::auth_validate_mnemonic,
            commands::auth_save_seed,
            commands::auth_load_seed,
            commands::auth_unlock,
            commands::send_file,
            commands::get_file_transfers,
            commands::open_file,
            commands::send_call_signal,
        ])
        .setup(|app| {
            commands::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

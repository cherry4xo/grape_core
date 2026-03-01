use std::path::PathBuf;
use std::sync::Arc;
use tauri::{App, Manager, Emitter};
use tokio::sync::Mutex;
use videocalls::{identity, network, storage, tauri_commands::*};

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,libp2p=debug")
        .init();

    // Initialize identity
    let identity_path = std::env::var("VIDEOCALLS_IDENTITY_PATH")
        .unwrap_or_else(|_| ".videocalls/identity.key".to_string());
    let identity_path = PathBuf::from(identity_path);

    let identity = identity::Identity::load_or_generate(&identity_path)?;
    let peer_id = identity.peer_id().to_string();
    tracing::info!("PeerId: {}", peer_id);

    let storage_path = std::env::var("VIDEOCALLS_STORAGE_PATH")
        .unwrap_or_else(|_| ".videocalls/storage".to_string());
    let storage_path = PathBuf::from(storage_path);

    let storage = Arc::new(storage::Storage::new(&storage_path)?);
    tracing::info!("📦 Storage initialized at {:?}", storage_path);

    let app_handle = app.handle().clone();
    let app_event_handle = app_handle.clone();

    tauri::async_runtime::spawn(async move {
        let mut network = match network::P2PNetwork::new(&identity, storage.clone()) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Failed to create P2P network: {:?}", e);
                return;
            }
        };

        if let Err(e) = network.listen("/ip4/0.0.0.0/tcp/0") {
            eprintln!("Failed to start listening: {:?}", e);
            return;
        }

        let command_tx = network.command_sender();
        let event_rx = network.event_receiver();

        // Store app state
        app_handle.manage(AppState {
            command_tx: command_tx.clone(),
            event_rx: Arc::new(Mutex::new(event_rx.resubscribe())),
            storage,
            peer_id,
        });

        // Event forwarding task
        let mut event_listener = event_rx.resubscribe();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = event_listener.recv().await {
                match &event {
                    network::NetworkEvent::PeerConnected { peer_id } => {
                        let _ = app_event_handle.emit("peer-connected", serde_json::json!({
                            "peer_id": peer_id.to_string()
                        }));
                    }
                    network::NetworkEvent::PeerDisconnected { peer_id } => {
                        let _ = app_event_handle.emit("peer-disconnected", serde_json::json!({
                            "peer_id": peer_id.to_string()
                        }));
                    }
                    network::NetworkEvent::MessageReceived { peer_id, message } => {
                        let _ = app_event_handle.emit("message-received", serde_json::json!({
                            "peer_id": peer_id.to_string(),
                            "message": message
                        }));
                    }
                    network::NetworkEvent::ChannelMessage { topic, peer_id, message } => {
                        let _ = app_event_handle.emit("channel-message", serde_json::json!({
                            "topic": topic,
                            "peer_id": peer_id.to_string(),
                            "message": message
                        }));
                    }
                    network::NetworkEvent::PeerDiscovered { peer_id, address } => {
                        let _ = app_event_handle.emit("peer-discovered", serde_json::json!({
                            "peer_id": peer_id.to_string(),
                            "address": address.to_string()
                        }));
                    }
                    network::NetworkEvent::ListenAddrAdded { address } => {
                        let _ = app_event_handle.emit("listen-addr-added", serde_json::json!({
                            "address": address.to_string()
                        }));
                    }
                }
            }
        });

        // Run network event loop
        if let Err(e) = network.run().await {
            eprintln!("Network error: {:?}", e);
        }
    });

    Ok(())
}

// Re-export all commands from the main project
pub use videocalls::tauri_commands::*;

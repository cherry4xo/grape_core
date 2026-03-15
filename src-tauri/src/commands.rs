use std::path::PathBuf;
use std::sync::Arc;
use tauri::{App, Manager, Emitter};
use tokio::sync::{oneshot, Mutex};
use videocalls::{identity, network, storage};

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,libp2p=debug")
        .init();

    let identity_path = PathBuf::from(
        std::env::var("VIDEOCALLS_IDENTITY_PATH")
            .unwrap_or_else(|_| ".videocalls/identity.key".to_string())
    );

    let seed_path = PathBuf::from(
        std::env::var("VIDEOCALLS_STORAGE_PATH")
            .unwrap_or_else(|_| ".videocalls".to_string())
    ).join("seed.enc");

    let storage_path = PathBuf::from(
        std::env::var("VIDEOCALLS_STORAGE_PATH")
            .unwrap_or_else(|_| ".videocalls/storage".to_string())
    );

    let storage = Arc::new(storage::Storage::new(&storage_path)?);
    tracing::info!("📦 Storage initialized at {:?}", storage_path);

    let (unlock_tx, unlock_rx) = oneshot::channel::<videocalls::tauri_commands::LibP2pKeypair>();
    app.manage(videocalls::tauri_commands::PendingAuth {
        unlock_tx: Arc::new(Mutex::new(Some(unlock_tx))),
    });

    let app_handle = app.handle().clone();

    enum IdentityResolution {
        Ready(identity::Identity),
        NeedsPassword(oneshot::Receiver<videocalls::tauri_commands::LibP2pKeypair>),
    }

    let resolution = if seed_path.exists() {
        match videocalls::auth::AuthManager::load(&seed_path, None) {
            Ok(auth) => match auth.derive_keypair() {
                Ok(kp) => IdentityResolution::Ready(identity::Identity::from_keypair(kp)),
                Err(_) => IdentityResolution::Ready(identity::Identity::load_or_generate(&identity_path)?),
            },
            Err(_) => IdentityResolution::NeedsPassword(unlock_rx),
        }
    } else {
        IdentityResolution::Ready(identity::Identity::load_or_generate(&identity_path)?)
    };

    tauri::async_runtime::spawn(async move {
        let identity = match resolution {
            IdentityResolution::Ready(id) => id,
            IdentityResolution::NeedsPassword(rx) => {
                let _ = app_handle.emit("auth-needs-password", ());
                tracing::info!("🔒 Waiting for password...");
                match rx.await {
                    Ok(kp) => identity::Identity::from_keypair(kp),
                    Err(_) => { eprintln!("Unlock channel closed"); return; }
                }
            }
        };

        let peer_id = identity.peer_id().to_string();
        tracing::info!("PeerId: {}", peer_id);

        let mut net = match network::P2PNetwork::new(&identity, storage.clone()) {
            Ok(n) => n,
            Err(e) => { eprintln!("Failed to create P2P network: {:?}", e); return; }
        };
        if let Err(e) = net.listen("/ip4/0.0.0.0/tcp/0") {
            eprintln!("Failed to listen: {:?}", e); return;
        }

        let command_tx = net.command_sender();
        let event_rx = net.event_receiver();

        app_handle.manage(AppState {
            command_tx,
            event_rx: Arc::new(tokio::sync::Mutex::new(event_rx.resubscribe())),
            storage,
            peer_id,
        });

        let mut listener = event_rx.resubscribe();
        let emitter = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = listener.recv().await {
                match &event {
                    network::NetworkEvent::PeerConnected { peer_id } => {
                        let _ = emitter.emit("peer-connected", serde_json::json!({"peer_id": peer_id.to_string()}));
                    }
                    network::NetworkEvent::PeerDisconnected { peer_id } => {
                        let _ = emitter.emit("peer-disconnected", serde_json::json!({"peer_id": peer_id.to_string()}));
                    }
                    network::NetworkEvent::MessageReceived { peer_id, message } => {
                        let _ = emitter.emit("message-received", serde_json::json!({"peer_id": peer_id.to_string(), "message": message}));
                    }
                    network::NetworkEvent::ChannelMessage { topic, peer_id, message } => {
                        let _ = emitter.emit("channel-message", serde_json::json!({"topic": topic, "peer_id": peer_id.to_string(), "message": message}));
                    }
                    network::NetworkEvent::PeerDiscovered { peer_id, address } => {
                        let _ = emitter.emit("peer-discovered", serde_json::json!({"peer_id": peer_id.to_string(), "address": address.to_string()}));
                    }
                    network::NetworkEvent::ListenAddrAdded { address } => {
                        let _ = emitter.emit("listen-addr-added", serde_json::json!({"address": address.to_string()}));
                    }
                    network::NetworkEvent::MessageDelivered { peer_id, message_id } => {
                        let _ = emitter.emit("message-delivered", serde_json::json!({
                            "peer_id": peer_id.to_string(),
                            "message_id": message_id
                        }));
                    }
                }
            }
        });

        if let Err(e) = net.run().await {
            eprintln!("Network error: {:?}", e);
        }
    });

    Ok(())
}

pub use videocalls::tauri_commands::*;
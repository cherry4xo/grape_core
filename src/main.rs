mod identity;
mod network;
mod protocol;
mod cli;
mod storage;
mod crypto;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,libp2p=debug")
        .init();

    info!("VideoCalls P2P Node");

    let identity_path = std::env::var("VIDEOCALLS_IDENTITY_PATH")
        .unwrap_or_else(|_| ".videocalls/identity.key".to_string());
    let identity_path = PathBuf::from(identity_path);

    let identity = identity::Identity::load_or_generate(&identity_path)?;
    info!("PeerId: {}", identity.peer_id());

    let storage_path = std::env::var("VIDEOCALLS_STORAGE_PATH")
        .unwrap_or_else(|_| ".videocalls/storage".to_string());
    let storage_path = PathBuf::from(storage_path);

    let storage = Arc::new(storage::Storage::new(&storage_path)?);
    info!("📦 Storage initialized at {:?}", storage_path);

    let mut network = network::P2PNetwork::new(&identity, storage.clone())?;
    network.listen("/ip4/0.0.0.0/tcp/0")?;

    let command_tx = network.command_sender();
    let event_rx = network.event_receiver();

    let cli_handle = tokio::spawn(async move {
        let mut cli = cli::CLI::new(command_tx, event_rx, storage);
        cli.run().await
    });

    let network_handle = tokio::spawn(async move {
        network.run().await
    });

    tokio::select! {
        result = cli_handle => {
            result??;
        }
        result = network_handle => {
            result??;
        }
    }

    Ok(())
}
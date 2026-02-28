mod identity;
mod network;

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber.fmt()
        .with_env_filter("info,libp2p=debug")
        .init();

    info!("VideoCalls P2P Node");

    let identity_path = PathBuf::from(".videocalls/identity.key");

    let identity = identity::Identity::load_or_generate(&identity_path)?;
    info!("PeerId: {}", identity.peer_id());

    let mut network = network::P2PNetwork::new(&identity)?;

    network.listen("/ip4/0.0.0.0/tcp/0")?;

    network.run().await?;

    Ok(())
}
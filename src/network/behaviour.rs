use libp2p::{
    mdns, ping,
    swarm::NetworkBehaviour,
    PeerId,
};
use anyhow::Result;

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub ping: ping::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
}

impl MyBehaviour {
    pub fn new(peer_id: PeerId) -> Result<Self> {
        Ok(Self {
            ping: ping::Behaviour::new(ping::Config::new()),
            mdns: mdns::tokio::Behaviour::new(
                mdns::Config::default(),
                peer_id
            )?,
        })
    }
}
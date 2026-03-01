use libp2p::{
    gossipsub, kad, mdns, ping, request_response,
    swarm::NetworkBehaviour,
    PeerId,
};
use anyhow::Result;
use crate::protocol::{self, MessageCodec};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub ping: ping::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub request_response: request_response::Behaviour<MessageCodec>,
    pub kademlia: kad::Behaviour<kad::store::MemoryStore>,
    pub gossipsub: gossipsub::Behaviour,
}

impl MyBehaviour {
    pub fn new(peer_id: PeerId) -> Result<Self> {
        let store = kad::store::MemoryStore::new(peer_id);
        let kademlia = kad::Behaviour::new(peer_id, store);

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(|message: &gossipsub::Message| {
                let mut hasher = DefaultHasher::new();
                message.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_string())
            })
            .build()
            .map_err(|e| anyhow::anyhow!("Gossipsub config error: {}", e))?;

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(
                libp2p::identity::Keypair::generate_ed25519()
            ),
            gossipsub_config,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create gossipsub client: {}", e))?;

        Ok(Self {
            ping: ping::Behaviour::new(ping::Config::new()),
            mdns: mdns::tokio::Behaviour::new(
                mdns::Config::default(),
                peer_id
            )?,
            request_response: protocol::create_behaviour(),
            kademlia,
            gossipsub,
        })
    }

    pub fn subscribe(&mut self, topic: &str) -> Result<bool> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.gossipsub
            .subscribe(&topic)
            .map_err(|e| anyhow::anyhow!("Subscribe error: {:?}", e))
    }

    pub fn unsubscribe(&mut self, topic: &str) -> Result<bool> {
        let topic = gossipsub::IdentTopic::new(topic);
        Ok(
            self.gossipsub.unsubscribe(&topic)
        )
    }

    pub fn publish(&mut self, topic: &str, data: Vec<u8>) -> Result<gossipsub::MessageId> {
        let topic = gossipsub::IdentTopic::new(topic);
        self.gossipsub
            .publish(topic, data)
            .map_err(|e| anyhow::anyhow!("Publish error: {:?}", e))
    }

    pub fn subscriptions(&self) -> Vec<String> {
        self.gossipsub
            .topics()
            .map(|t| t.to_string())
            .collect()
    }

    pub fn add_bootstrap_peer(&mut self, peer_id: PeerId, addr: libp2p::Multiaddr) {
        self.kademlia.add_address(&peer_id, addr);
    }

    pub fn bootstrap(&mut self) -> Result<kad::QueryId> {
        self.kademlia
            .bootstrap()
            .map_err(|e| anyhow::anyhow!("Bootstrap failed: {:?}", e))
    }
}
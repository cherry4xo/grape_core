use libp2p::{
    gossipsub, kad, mdns, ping, request_response, autonat, identify,
    swarm::NetworkBehaviour,
    PeerId,
};
use anyhow::Result;
use crate::protocol::{self, MessageCodec};
use crate::network::kad_store::PersistentStore;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub ping: ping::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub request_response: request_response::Behaviour<MessageCodec>,
    pub kademlia: kad::Behaviour<PersistentStore>,
    pub gossipsub: gossipsub::Behaviour,
    pub autonat: autonat::Behaviour,
    pub identify: identify::Behaviour,
}

impl MyBehaviour {
    pub fn new(
        peer_id: PeerId,
        keypair: &libp2p::identity::Keypair,
    ) -> Result<Self> {
        let storage_path = std::env::var("VIDEOCALLS_STORAGE_PATH")
            .unwrap_or_else(|_| ".videocalls/storage".to_string());
        let kad_store_path = std::path::PathBuf::from(storage_path).join("kad_store");

        let _ = std::fs::create_dir_all(&kad_store_path);

        let store = PersistentStore::new(kad_store_path, peer_id)
            .map_err(|e| anyhow::anyhow!("Failed to create persistent kad store: {}", e))?;
        let mut kademlia = kad::Behaviour::new(peer_id, store);

        kademlia.set_mode(Some(kad::Mode::Server));

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
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create gossipsub client: {}", e))?;

        let autonat = autonat::Behaviour::new(
            peer_id,
            autonat::Config {
                retry_interval: Duration::from_secs(90),
                refresh_interval: Duration::from_secs(30 * 60),
                boot_delay: Duration::from_secs(5),
                throttle_server_period: Duration::ZERO,
                only_global_ips: true,
                ..Default::default()
            },
        );

        let identify = identify::Behaviour::new(
            identify::Config::new(
                "/videocalls/1.0.0".to_string(),
                keypair.public(),
            )
            .with_push_listen_addr_updates(true)
            .with_interval(Duration::from_secs(5 * 60))
        );

        Ok(Self {
            ping: ping::Behaviour::new(ping::Config::new()),
            mdns: mdns::tokio::Behaviour::new(
                mdns::Config::default(),
                peer_id
            )?,
            request_response: protocol::create_behaviour(),
            kademlia,
            gossipsub,
            autonat,
            identify,
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

    pub fn get_closest_peers(&mut self, peer_id: PeerId) -> kad::QueryId {
        self.kademlia.get_closest_peers(peer_id)
    }
}
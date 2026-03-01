pub mod behaviour;
pub mod transport;
pub mod bootstrap;

use std::collections::HashMap;

use crate::identity::Identity;
use crate::protocol::Message;
use crate::crypto::MessageEncryption;
use crate::storage::{Storage, StoredMessage};
use anyhow::Result;
use std::sync::Arc;
use libp2p::{Multiaddr, PeerId, Swarm, gossipsub, kad, request_response};
use libp2p::request_response::OutboundRequestId;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub enum NetworkCommand {
    SendMessage { peer_id: PeerId, message: String },
    Dial { address: Multiaddr },
    GetPeers,
    SubscribeChannel { topic: String },
    UnsubscribeChannel { topic: String },
    PublishToChannel { topic: String, message: String },
    ListChannels,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerDiscovered { peer_id: PeerId, address: Multiaddr },
    PeerConnected { peer_id: PeerId },
    PeerDisconnected { peer_id: PeerId },
    MessageReceived { peer_id: PeerId, message: String },
    ChannelMessage { topic: String, peer_id: PeerId, message: String},
    ListenAddrAdded { address: Multiaddr },
}

pub struct P2PNetwork {
    swarm: Swarm<behaviour::MyBehaviour>,
    command_rx: mpsc::Receiver<NetworkCommand>,
    command_tx: mpsc::Sender<NetworkCommand>,
    event_tx: broadcast::Sender<NetworkEvent>,
    pending_requests: HashMap<OutboundRequestId, PeerId>,
    encryption: MessageEncryption,
    storage: Arc<Storage>,
    local_peer_id: PeerId,
}

impl P2PNetwork {
    pub fn new(identity: &Identity, storage: Arc<Storage>) -> Result<Self> {
        let keypair = identity.keypair().clone();
        let peer_id = *identity.peer_id();

        let transport = transport::build_transport(&keypair)?;
        let behaviour = behaviour::MyBehaviour::new(peer_id)?;

        let swarm = Swarm::new(
            transport,
            behaviour,
            peer_id,
            libp2p::swarm::Config::with_tokio_executor()
                .with_idle_connection_timeout(std::time::Duration::from_secs(3600))
                .with_max_negotiating_inbound_streams(128),
        );

        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, _) = broadcast::channel(100);
        let pending_requests = HashMap::new();

        let encryption = MessageEncryption::new(keypair.clone());

        Ok(Self {
            swarm,
            command_rx,
            command_tx,
            event_tx,
            pending_requests,
            encryption,
            storage,
            local_peer_id: peer_id
        })
    }

    pub fn command_sender(&self) -> mpsc::Sender<NetworkCommand> {
        self.command_tx.clone()
    }

    pub fn event_receiver(&self) -> broadcast::Receiver<NetworkEvent> {
        self.event_tx.subscribe()
    }

    pub fn listen(&mut self, addr: &str) -> Result<()> {
        let multiaddr: Multiaddr = addr.parse()?;
        self.swarm.listen_on(multiaddr)?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        use libp2p::{mdns, swarm::SwarmEvent};
        use futures::StreamExt;

        info!("🌐 Network started");

        let mut keys_exchanged: HashMap<PeerId, bool> = HashMap::new();

        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
                    match command {
                        NetworkCommand::Shutdown => {
                            info!("🛑 Shutting down network");
                            break;
                        }
                        NetworkCommand::Dial { address } => {
                            debug!("📞 Dialing {}", address);
                            if let Err(e) = self.swarm.dial(address.clone()) {
                                warn!("❌ Failed to dial {}: {:?}", address, e);
                            }
                        }
                        NetworkCommand::GetPeers => {
                            let peers: Vec<_> = self.swarm.connected_peers().collect();
                            info!("👥 Connected peers: {:?}", peers);
                        }
                        NetworkCommand::SendMessage { peer_id, message } => {
                            info!("📤 Sending message to {}: {}", peer_id, message);

                            let stored_msg = StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                chat_id: peer_id.to_string(),
                                sender: self.local_peer_id.to_string(),
                                content: message.clone(),
                                timestamp: chrono::Utc::now().timestamp(),
                                is_outgoing: true,
                            };

                            if let Err(e) = self.storage.save_message(&stored_msg) {
                                warn!("❌ Failed to save outgoing message: {:?}", e);
                            }

                            if keys_exchanged.get(&peer_id).copied().unwrap_or(false) {
                                match self.encryption.encrypt(&peer_id, message.as_bytes()) {
                                    Ok(ciphertext) => {
                                        let msg = Message::EncryptedText {
                                            ciphertext,
                                            timestamp: chrono::Utc::now().timestamp(),
                                        };

                                        info!("🔒 Sending encrypted message to {}", peer_id);
                                        let request_id = self.swarm
                                            .behaviour_mut()
                                            .request_response
                                            .send_request(&peer_id, msg);

                                        self.pending_requests.insert(request_id, peer_id);
                                    }
                                    Err(e) => {
                                        warn!("❌ Encryption failed: {:?}", e);
                                    }
                                }
                            } else {
                                warn!("⚠️ Key exchange not completed, sending unencrypted");

                                let msg = Message::Text {
                                    context: message,
                                    timestamp: chrono::Utc::now().timestamp(),
                                };

                                let request_id = self.swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_request(&peer_id, msg);

                                self.pending_requests.insert(request_id, peer_id);
                            }
                        }
                        NetworkCommand::SubscribeChannel { topic } => {
                            info!("Subscribing to channel: {}", topic);
                            if let Err(e) = self.swarm.behaviour_mut().subscribe(&topic) {
                                warn!("Failed to subscribe: {:?}", e);
                            }
                        }

                        NetworkCommand::UnsubscribeChannel { topic } => {
                            info!("Unsubscribing from channel: {}", topic);
                            if let Err(e) = self.swarm.behaviour_mut().unsubscribe(&topic) {
                                warn!("Failed to unsubscribe: {:?}", e);
                            }
                        }

                        NetworkCommand::PublishToChannel { topic, message } => {
                            info!("Publishing to {}: {}", topic, message);
                            let data = message.as_bytes().to_vec();
                            if let Err(e) = self.swarm.behaviour_mut().publish(&topic, data) {
                                warn!("Failed to publish: {:?}", e);
                            }
                        }

                        NetworkCommand::ListChannels => {
                            let subscriptions = self.swarm.behaviour().subscriptions();
                            info!("Subscribed channels: {:?}", subscriptions);
                        }
                    }
                }

                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("📡 Listening on: {}", address);
                            let _ = self.event_tx.send(NetworkEvent::ListenAddrAdded { address });
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                            for (peer_id, addr) in peers {
                                if peer_id == *self.swarm.local_peer_id() {
                                    debug!("🔄 Ignoring own address: {}", addr);
                                    continue;
                                }

                                info!("🔍 Discovered peer: {} at {}", peer_id, addr);
                                let _ = self.event_tx.send(NetworkEvent::PeerDiscovered { peer_id, address: addr.clone() });

                                if !self.swarm.is_connected(&peer_id) {
                                    if let Err(e) = self.swarm.dial(addr) {
                                        warn!("❌ Failed to dial peer: {:?}", e);
                                    }
                                }
                            }
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                            for (peer_id, _) in peers {
                                info!("😴 Peer expired: {}", peer_id);
                            }
                        }

                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, ..} => {
                            info!("✅ Connected to {} at {}", peer_id, endpoint.get_remote_address());
                            let _ = self.event_tx.send(NetworkEvent::PeerConnected { peer_id });

                            if let Err(e) = self.storage.update_last_seen(&peer_id, chrono::Utc::now().timestamp()) {
                                warn!("❌ Failed to save contact: {:?}", e);
                            } else {
                                info!("💾 Contact saved: {}", peer_id);
                            }

                            if !keys_exchanged.contains_key(&peer_id) {
                                let my_public_key = self.encryption.public_key();

                                let key_exchange_msg = Message::KeyExchange {
                                    public_key: my_public_key,
                                };

                                info!("🔑 Sending public key to {}", peer_id);
                                let request_id = self.swarm
                                    .behaviour_mut()
                                    .request_response
                                    .send_request(&peer_id, key_exchange_msg);

                                self.pending_requests.insert(request_id, peer_id);
                            }
                        }

                        SwarmEvent::ConnectionClosed { peer_id, cause, ..} => {
                            info!("❌ Disconnected from {} (cause {:?})", peer_id, cause);
                            let _ = self.event_tx.send(NetworkEvent::PeerDisconnected { peer_id });
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Ping(event)) => {
                            debug!("🏓 Ping event: {:?}", event);
                        }

                        SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                            debug!("📥 Incoming connection from {}", send_back_addr);
                        }

                        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                            if let Some(peer) = peer_id {
                                warn!("⚠️ Outgoing connection to {} failed: {:?}", peer, error);
                            } else {
                                warn!("⚠️ Outgoing connection failed: {:?}", error);
                            }
                        }

                        SwarmEvent::IncomingConnectionError { send_back_addr, error, .. } => {
                            warn!("⚠️ Incoming connection from {} failed: {:?}", send_back_addr, error);
                        }
                        
                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::RequestResponse(
                            request_response::Event::Message { peer, message, .. }
                        )) => {
                            match message {
                                request_response::Message::Request { request, channel, .. } => {
                                    match &request {
                                        Message::KeyExchange { public_key } => {
                                            info!("🔑 Received public key from {}", peer);

                                            if let Err(e) = self.encryption.add_peer(peer, public_key) {
                                                warn!("❌ Failed to add peer key: {:?}", e);
                                            } else {
                                                info!("✅ Key exchange completed with {}", peer);
                                                keys_exchanged.insert(peer, true);

                                                let my_public_key = self.encryption.public_key();
                                                let response = Message::KeyExchange {
                                                    public_key: my_public_key,
                                                };
                                                
                                                let _ = self.swarm
                                                    .behaviour_mut()
                                                    .request_response
                                                    .send_response(channel, response);
                                            }
                                        }

                                        Message::Text { context, timestamp } => {
                                            info!("📨 Received request from {}: {:?}", peer, request);

                                            let stored_msg = StoredMessage {
                                                id: uuid::Uuid::new_v4().to_string(),
                                                chat_id: peer.to_string(),
                                                sender: peer.to_string(),
                                                content: context.clone(),
                                                timestamp: *timestamp,
                                                is_outgoing: false,
                                            };

                                            if let Err(e) = self.storage.save_message(&stored_msg) {
                                                warn!("❌ Failed to save incoming message: {:?}", e);
                                            }

                                            let _ = self.event_tx.send(NetworkEvent::MessageReceived {
                                                peer_id: peer,
                                                message: context.clone(),
                                            });

                                            let response = Message::Pong;
                                            let _ = self.swarm
                                                .behaviour_mut()
                                                .request_response
                                                .send_response(channel, response);
                                        }

                                        Message::EncryptedText { ciphertext, timestamp } => {
                                            info!("🔒 Received encrypted message from {}", peer);

                                            match self.encryption.decrypt(&peer, ciphertext) {
                                                Ok(plaintext) => {
                                                    let message = String::from_utf8_lossy(&plaintext).to_string();
                                                    info!("✅ Decrypted: {}", message);

                                                    let stored_msg = StoredMessage {
                                                        id: uuid::Uuid::new_v4().to_string(),
                                                        chat_id: peer.to_string(),
                                                        sender: peer.to_string(),
                                                        content: message.clone(),
                                                        timestamp: *timestamp,
                                                        is_outgoing: false,
                                                    };

                                                    if let Err(e) = self.storage.save_message(&stored_msg) {
                                                        warn!("❌ Failed to save incoming message: {:?}", e);
                                                    }
                                                    
                                                    let _ = self.event_tx.send(NetworkEvent::MessageReceived {
                                                        peer_id: peer,
                                                        message,
                                                    });
                                                }
                                                Err(e) => {
                                                    warn!("❌ Decryption failed: {:?}", e);
                                                }
                                            }

                                            let response = Message::Pong;
                                            let _ = self.swarm
                                                .behaviour_mut()
                                                .request_response
                                                .send_response(channel, response);
                                        }

                                        _ => {
                                            let response = Message::Pong;
                                            let _ = self.swarm
                                                .behaviour_mut()
                                                .request_response
                                                .send_response(channel, response);
                                        }
                                    }    
                                }

                                request_response::Message::Response { response, .. } => {
                                    match response {
                                        Message::KeyExchange { public_key } => {
                                            info!("🔑 Received public key response from {}", peer);

                                            if let Err(e) = self.encryption.add_peer(peer, &public_key) {
                                                warn!("❌ Failed to add peer key: {:?}", e);
                                            } else {
                                                info!("✅ Key exchange completed with {}", peer);
                                                keys_exchanged.insert(peer, true);
                                            }
                                        }
                                        _ => {
                                            info!("📬 Received response from {}: {:?}", peer, response);
                                        }
                                    }
                                }
                            }
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Kademlia(event)) => {
                            match event {
                                kad::Event::RoutingUpdated { peer, .. } => {
                                    debug!("Routing updated for peer: {}", peer);
                                }
                                kad::Event::RoutablePeer { peer, address } => {
                                    debug!("Routable peer: {} at {}", peer, address);
                                }
                                kad::Event::UnroutablePeer { peer } => {
                                    debug!("Unroutable peer: {}", peer);
                                }
                                kad::Event::OutboundQueryProgressed { result, .. } => {
                                    match result {
                                        kad::QueryResult::Bootstrap(Ok(kad::BootstrapOk { peer, num_remaining })) => {
                                            info!("Bootstrap succeeded with peer {}, {} remaining", peer, num_remaining);
                                        }
                                        kad::QueryResult::Bootstrap(Err(kad::BootstrapError::Timeout { .. })) => {
                                            warn!("Bootstrap timeout");
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Gossipsub(
                            gossipsub::Event::Message { propagation_source, message_id: _, message }
                        )) => {
                            let topic = message.topic.to_string();
                            let peer_id = propagation_source;
                            let msg = String::from_utf8_lossy(&message.data).to_string();

                            info!("📢 Channel message from {} in {}: {}", peer_id, topic, msg);

                            let _ = self.event_tx.send(NetworkEvent::ChannelMessage {
                                topic,
                                peer_id,
                                message: msg,
                            });
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Gossipsub(
                            gossipsub::Event::Subscribed { peer_id, topic }
                        )) => {
                            info!("Peer {} subscribed to {}", peer_id, topic);
                        }

                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }
}
pub mod behaviour;
pub mod transport;
pub mod bootstrap;
pub mod kad_store;

use std::collections::HashMap;

use crate::identity::Identity;
use crate::protocol::Message;
use crate::crypto::MessageEncryption;
use crate::storage::{FileTransfer, Storage, StoredMessage};
use anyhow::Result;
use std::sync::Arc;
use libp2p::{Multiaddr, PeerId, Swarm, autonat, gossipsub, identify, kad, request_response};
use libp2p::request_response::OutboundRequestId;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub enum NetworkCommand {
    SendMessage { peer_id: PeerId, message: String },
    SendFile { peer_id: PeerId, file_path: std::path::PathBuf, caption: String },
    SendCallSignal {
        peer_id: PeerId,
        call_id: String,
        signal_json: String,
    },
    Dial { address: Multiaddr },
    GetPeers,
    SubscribeChannel { topic: String },
    UnsubscribeChannel { topic: String },
    PublishToChannel { topic: String, message: String },
    ListChannels,
    Bootstrap,
    FindPeer { peer_id: PeerId },
    GetProviders { key: String },
    GetDhtStats,
    GetNatStatus,
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
    MessageDelivered { peer_id: PeerId, message_id: String },
    FileReceived {
        peer_id: PeerId,
        transfer_id: String,
        message_id: String,
        file_name: String,
        file_path: String,
        size: u64,
        mime_type: Option<String>,
        caption: String,
    },
    FileTransferProgress {
        peer_id: PeerId,
        transfer_id: String,
        chunks_done: u32,
        total_chunks: u32,
    },
    FileTransferFailed {
        peer_id: PeerId,
        transfer_id: String,
        reason: String,
    },
    FileTransferCompleted {
        peer_id: PeerId,
        transfer_id: String,
    },
    CallSignalReceived {
        peer_id: PeerId,
        call_id: String,
        signal_json: String,
    },
}

pub struct P2PNetwork {
    swarm: Swarm<behaviour::MyBehaviour>,
    command_rx: mpsc::Receiver<NetworkCommand>,
    command_tx: mpsc::Sender<NetworkCommand>,
    event_tx: broadcast::Sender<NetworkEvent>,
    pending_requests: HashMap<OutboundRequestId, (PeerId, Option<String>)>,
    encryption: MessageEncryption,
    storage: Arc<Storage>,
    local_peer_id: PeerId,
    pending_outgoing_chunks: HashMap<String, (PeerId, Vec<Vec<u8>>, u32)>,
    pending_incoming_chunks: HashMap<String, (FileTransfer, HashMap<u32, Vec<u8>>)>,
    pending_file_requests: HashMap<OutboundRequestId, (PeerId, String, u32)>,
    downloads_dir: std::path::PathBuf,
}

impl P2PNetwork {
    pub fn new(identity: &Identity, storage: Arc<Storage>) -> Result<Self> {
        let keypair = identity.keypair().clone();
        let peer_id = *identity.peer_id();

        let transport = transport::build_transport(&keypair)?;
        let mut behaviour = behaviour::MyBehaviour::new(peer_id, &keypair)?;

        bootstrap::setup_bootstrap(&mut behaviour);

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

        let storage_base = std::env::var("VIDEOCALLS_STORAGE_PATH")
            .unwrap_or_else(|_| "./videocalls/storage".to_string());
        let downloads_dir = std::path::PathBuf::from(storage_base).join("downloads");

        Ok(Self {
            swarm,
            command_rx,
            command_tx,
            event_tx,
            pending_requests,
            encryption,
            storage,
            local_peer_id: peer_id,
            pending_outgoing_chunks: HashMap::new(),
            pending_incoming_chunks: HashMap::new(),
            pending_file_requests: HashMap::new(),
            downloads_dir,
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

        let bootstrap_peers: std::collections::HashSet<PeerId> =
            bootstrap::get_bootstrap_nodes().into_iter().map(|n| n.peer_id).collect();

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

                            let msg_id = uuid::Uuid::new_v4().to_string();
                            let is_connected = self.swarm.is_connected(&peer_id);
                            let status = if is_connected { "sent" } else { "queued" };

                            let stored_msg = StoredMessage {
                                id: msg_id.clone(),
                                chat_id: peer_id.to_string(),
                                sender: self.local_peer_id.to_string(),
                                content: message.clone(),
                                timestamp: chrono::Utc::now().timestamp(),
                                is_outgoing: true,
                                delivery_status: status.to_string(),
                                file_transfer_id: None,
                            };
                            if let Err(e) = self.storage.save_message(&stored_msg) {
                                warn!("Failed to save message: {:?}", e);
                            }

                            if !is_connected {
                                if let Err(e) = self.storage.save_pending_message(&peer_id.to_string(), &message, &msg_id) {
                                    warn!("Failed to queue offline message: {:?}", e);
                                }
                                info!("📬 Queued message for offline peer {}", peer_id);
                                // skip send
                            } else if keys_exchanged.get(&peer_id).copied().unwrap_or(false) {
                                match self.encryption.encrypt(&peer_id, message.as_bytes()) {
                                    Ok(ciphertext) => {
                                        let msg = Message::EncryptedText { ciphertext, timestamp: chrono::Utc::now().timestamp() };
                                        let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
                                        self.pending_requests.insert(request_id, (peer_id, Some(msg_id)));
                                    }
                                    Err(e) => warn!("Encryption failed: {:?}", e),
                                }
                            } else {
                                warn!("⚠️ Key exchange not completed, sending unencrypted");
                                let msg = Message::Text { context: message, timestamp: chrono::Utc::now().timestamp() };
                                let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
                                self.pending_requests.insert(request_id, (peer_id, Some(msg_id)));
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

                        NetworkCommand::Bootstrap => {
                            info!("🚀 Triggering manual bootstrap...");
                            match self.swarm.behaviour_mut().bootstrap() {
                                Ok(query_id) => {
                                    info!("✅ Bootstrap query started: {:?}", query_id);
                                }
                                Err(e) => {
                                    warn!("❌ Bootstrap failed: {:?}", e);
                                }
                            }
                        }

                        NetworkCommand::FindPeer { peer_id } => {
                            info!("🔍 Finding peer {} in DHT...", peer_id);
                            let query_id = self.swarm.behaviour_mut().get_closest_peers(peer_id);
                            info!("📡 DHT query started: {:?}", query_id);
                        }

                        NetworkCommand::GetProviders { key } => {
                            info!("🔍 Getting providers for key: {}", key);
                            // TODO: Implement kad::get_providers when needed
                            warn!("⚠️ GetProviders not yet implemented");
                        }

                        NetworkCommand::GetDhtStats => {
                            let mut kad_peers = Vec::new();
                            for bucket in self.swarm.behaviour_mut().kademlia.kbuckets() {
                                for entry in bucket.iter() {
                                    kad_peers.push(entry.node.key.preimage().clone());
                                }
                            }

                            info!("📊 DHT Routing Table:");
                            info!("  Total peers in routing table: {}", kad_peers.len());
                            info!("  Connected peers: {}", self.swarm.connected_peers().count());

                            if !kad_peers.is_empty() {
                                info!("  Peers in DHT:");
                                for peer in kad_peers.iter().take(10) {
                                    info!("    - {}", peer);
                                }
                                if kad_peers.len() > 10 {
                                    info!("    ... and {} more", kad_peers.len() - 10);
                                }
                            }
                        }

                        NetworkCommand::GetNatStatus => {
                            info!("🌐 NAT status information is logged via Autonat events");
                            info!("   Watch for 'NAT status changed' messages in the logs");
                        }

                        NetworkCommand::SendFile { peer_id, file_path, caption } => {
                            let file_name = file_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();

                            let mime_type = mime_guess::from_path(&file_path)
                                .first()
                                .map(|m| m.to_string());

                            let bytes = match tokio::fs::read(&file_path).await {
                                Ok(b) => b,
                                Err(e) => {
                                    warn!("Failed to read file {:?}: {:?}", file_path, e);
                                    continue;
                                }
                            };

                            let transfer_id = uuid::Uuid::new_v4().to_string();
                            let message_id = uuid::Uuid::new_v4().to_string();
                            let file_size = bytes.len() as u64;
                            let use_encryption = keys_exchanged.get(&peer_id).copied().unwrap_or(false);

                            // Split and encrypt each chunk with its index as nonce
                            let mut encrypted_chunks: Vec<Vec<u8>> = Vec::new();
                            for (i, chunk) in bytes.chunks(crate::protocol::FILE_CHUNK_SIZE).enumerate() {
                                let encrypted = if use_encryption {
                                    match self.encryption.encrypt_chunk(&peer_id, chunk, i as u64) {
                                        Ok(c) => c,
                                        Err(e) => {
                                            warn!("Chunk encryption failed: {:?}", e);
                                            chunk.to_vec()
                                        }
                                    }
                                } else {
                                    chunk.to_vec()
                                };
                                encrypted_chunks.push(encrypted);
                            }

                            let total_chunks = encrypted_chunks.len() as u32;
                            let is_connected = self.swarm.is_connected(&peer_id);
                            let status = if is_connected { "in_progress" } else { "queued" };

                            let stored_msg = StoredMessage {
                                id: message_id.clone(),
                                chat_id: peer_id.to_string(),
                                sender: self.local_peer_id.to_string(),
                                content: caption.clone(),
                                timestamp: chrono::Utc::now().timestamp(),
                                is_outgoing: true,
                                delivery_status: status.to_string(),
                                file_transfer_id: Some(transfer_id.clone()),
                            };
                            let _ = self.storage.save_message(&stored_msg);

                            let ft = crate::storage::FileTransfer {
                                id: transfer_id.clone(),
                                chat_id: peer_id.to_string(),
                                file_name: file_name.clone(),
                                file_size,
                                total_chunks,
                                chunks_done: 0,
                                local_path: None,
                                is_outgoing: true,
                                status: status.to_string(),
                                timestamp: chrono::Utc::now().timestamp(),
                                mime_type: mime_type.clone(),
                            };
                            let _ = self.storage.save_file_transfer(&ft);

                            if !is_connected {
                                let _ = self.storage.save_pending_file_transfer(
                                    &peer_id.to_string(),
                                    &transfer_id,
                                    &file_path.to_string_lossy(),
                                    &caption,
                                );
                                info!("📬 Queued file transfer for offline peer {}", peer_id);
                            } else {
                                let msg = crate::protocol::Message::FileOffer {
                                    transfer_id: transfer_id.clone(),
                                    file_name,
                                    file_size,
                                    total_chunks,
                                    mime_type,
                                    caption,
                                    message_id,
                                };
                                let request_id = self.swarm.behaviour_mut().request_response
                                    .send_request(&peer_id, msg);
                                self.pending_file_requests.insert(request_id, (peer_id, transfer_id.clone(), u32::MAX));
                                self.pending_outgoing_chunks.insert(transfer_id, (peer_id, encrypted_chunks, 0));
                                info!("📤 Sent FileOffer to {}", peer_id);
                            }
                        }

                        NetworkCommand::SendCallSignal { peer_id, call_id, signal_json } => {
                            let msg = crate::protocol::Message::CallSignal { call_id, signal_json };
                            self.swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
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

                            if !bootstrap_peers.contains(&peer_id) {
                                let _ = self.event_tx.send(NetworkEvent::PeerConnected { peer_id });
                                if let Err(e) = self.storage.save_seen_peer(&peer_id, chrono::Utc::now().timestamp()) {
                                    warn!("❌ Failed to save seen peer: {:?}", e);
                                }
                                if let Err(e) = self.storage.mark_peer_messages_delivered(&peer_id.to_string()) {
                                    warn!("❌ Failed to mark messages delivered: {:?}", e);
                                }
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

                                self.pending_requests.insert(request_id, (peer_id, None));
                            }

                            let pending = self.storage.get_pending_messages(&peer_id.to_string()).unwrap_or_default();
                            for (msg_id, content) in pending {
                                let msg = if keys_exchanged.get(&peer_id).copied().unwrap_or(false) {
                                    match self.encryption.encrypt(&peer_id, content.as_bytes()) {
                                        Ok(ciphertext) => Message::EncryptedText { ciphertext, timestamp: chrono::Utc::now().timestamp() },
                                        Err(_) => Message::Text { context: content.clone(), timestamp: chrono::Utc::now().timestamp() },
                                    }
                                } else {
                                    Message::Text { context: content.clone(), timestamp: chrono::Utc::now().timestamp() }
                                };
                                let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, msg);
                                self.pending_requests.insert(request_id, (peer_id, Some(msg_id.clone())));
                                let _ = self.storage.delete_pending_message(&peer_id.to_string(), &msg_id);
                                let _ = self.storage.update_message_status(&msg_id, "sent");
                            }

                            let pending_files = self.storage.get_pending_file_transfers(&peer_id.to_string())
                                .unwrap_or_default();
                            for (transfer_id, file_path_str, caption) in pending_files {
                                let file_path = std::path::PathBuf::from(&file_path_str);
                                let bytes = match tokio::fs::read(&file_path).await {
                                    Ok(b) => b,
                                    Err(e) => {
                                        warn!("❌ Failed to re-read queued file {:?}: {:?}", file_path, e);
                                        let _ = self.storage.update_file_transfer_status(&transfer_id, "failed");
                                        let _ = self.storage.delete_pending_file_transfer(&peer_id.to_string(), &transfer_id);
                                        continue;
                                    }
                                };

                                let use_encryption = keys_exchanged.get(&peer_id).copied().unwrap_or(false);
                                let mut encrypted_chunks: Vec<Vec<u8>> = Vec::new();

                                for (i, chunk) in bytes.chunks(crate::protocol::FILE_CHUNK_SIZE).enumerate() {
                                    let c = if use_encryption {
                                        self.encryption.encrypt_chunk(&peer_id, chunk, i as u64).unwrap_or_else(|_| chunk.to_vec())
                                    } else {
                                        chunk.to_vec()
                                    };
                                    encrypted_chunks.push(c);
                                }

                                let total_chunks = encrypted_chunks.len() as u32;

                                if let Ok(Some(ft)) = self.storage.get_file_transfer(&transfer_id) {
                                    // Find the message_id linked to this transfer
                                    let message_id = self.storage.get_messages(&ft.chat_id, 200)
                                        .unwrap_or_default()
                                        .into_iter()
                                        .find(|m| m.file_transfer_id.as_deref() == Some(&transfer_id))
                                        .map(|m| m.id)
                                        .unwrap_or_else(|| transfer_id.clone());
                                    let msg = crate::protocol::Message::FileOffer {
                                        transfer_id: transfer_id.clone(),
                                        file_name: ft.file_name.clone(),
                                        file_size: ft.file_size,
                                        total_chunks,
                                        mime_type: ft.mime_type.clone(),
                                        caption,
                                        message_id,
                                    };
                                    let request_id = self.swarm.behaviour_mut().request_response
                                        .send_request(&peer_id, msg);
                                    self.pending_file_requests.insert(request_id, (peer_id, transfer_id.clone(), u32::MAX));
                                    self.pending_outgoing_chunks.insert(transfer_id.clone(), (peer_id, encrypted_chunks, 0));
                                    let _ = self.storage.update_file_transfer_status(&transfer_id, "in_progress");
                                    let _ = self.storage.delete_pending_file_transfer(&peer_id.to_string(), &transfer_id);
                                }
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
                                                delivery_status: "delivered".to_string(),
                                                file_transfer_id: None,
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
                                                        delivery_status: "delivered".to_string(),
                                                        file_transfer_id: None,
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

                                        Message::FileOffer { transfer_id, file_name, file_size, total_chunks, mime_type, caption, message_id } => {
                                            info!("📥 FileOffer from {}: {} ({} bytes, {} chunks)", peer, file_name, file_size, total_chunks);

                                            let stored_msg = StoredMessage {
                                                id: message_id.clone(),
                                                chat_id: peer.to_string(),
                                                sender: peer.to_string(),
                                                content: caption.clone(),
                                                timestamp: chrono::Utc::now().timestamp(),
                                                is_outgoing: false,
                                                delivery_status: "delivered".to_string(),
                                                file_transfer_id: Some(transfer_id.clone()),
                                            };
                                            let _ = self.storage.save_message(&stored_msg);

                                            let ft = crate::storage::FileTransfer {
                                                id: transfer_id.clone(),
                                                chat_id: peer.to_string(),
                                                file_name: file_name.clone(),
                                                file_size: *file_size,
                                                total_chunks: *total_chunks,
                                                chunks_done: 0,
                                                local_path: None,
                                                is_outgoing: false,
                                                status: "in_progress".to_string(),
                                                timestamp: chrono::Utc::now().timestamp(),
                                                mime_type: mime_type.clone(),
                                            };
                                            let _ = self.storage.save_file_transfer(&ft);
                                            self.pending_incoming_chunks.insert(
                                                transfer_id.clone(),
                                                (ft, HashMap::new()),
                                            );

                                            let _ = self.event_tx.send(NetworkEvent::FileTransferProgress {
                                                peer_id: peer,
                                                transfer_id: transfer_id.clone(),
                                                chunks_done: 0,
                                                total_chunks: *total_chunks,
                                            });

                                            let _ = self.swarm.behaviour_mut().request_response
                                                .send_response(channel, Message::Pong);
                                        }

                                        Message::FileChunk { transfer_id, chunk_index, data } => {
                                            if let Some((ft, chunks_map)) = self.pending_incoming_chunks.get_mut(transfer_id) {
                                                // Decrypt using chunk_index as nonce (matches sender's encrypt_chunk)
                                                let decrypted = if keys_exchanged.get(&peer).copied().unwrap_or(false) {
                                                    match self.encryption.decrypt_chunk(&peer, data, *chunk_index as u64) {
                                                        Ok(d) => d,
                                                        Err(e) => {
                                                            warn!("Chunk decryption failed: {:?}", e);
                                                            data.clone()
                                                        }
                                                    }
                                                } else {
                                                    data.clone()
                                                };
                                                chunks_map.insert(*chunk_index, decrypted);

                                                let chunks_done = chunks_map.len() as u32;
                                                let total_chunks = ft.total_chunks;
                                                let transfer_id_clone = transfer_id.clone();

                                                let _ = self.storage.update_file_transfer_progress(transfer_id, chunks_done);
                                                let _ = self.event_tx.send(NetworkEvent::FileTransferProgress {
                                                    peer_id: peer,
                                                    transfer_id: transfer_id_clone,
                                                    chunks_done,
                                                    total_chunks,
                                                });
                                            }

                                            let _ = self.swarm.behaviour_mut().request_response
                                                .send_response(channel, Message::FileAck {
                                                    transfer_id: transfer_id.clone(),
                                                    chunk_index: *chunk_index,
                                                });
                                        }

                                        Message::FileComplete { transfer_id } => {
                                            if let Some((ft, chunks_map)) = self.pending_incoming_chunks.remove(transfer_id) {
                                                // Assemble chunks in order
                                                let mut assembled: Vec<u8> = Vec::with_capacity(ft.file_size as usize);
                                                for i in 0..ft.total_chunks {
                                                    if let Some(chunk) = chunks_map.get(&i) {
                                                        assembled.extend_from_slice(chunk);
                                                    } else {
                                                        warn!("Missing chunk {} for transfer {}", i, transfer_id);
                                                    }
                                                }

                                                // Write to disk
                                                let out_dir = self.downloads_dir.join(transfer_id);
                                                let out_path = out_dir.join(&ft.file_name);
                                                match tokio::fs::create_dir_all(&out_dir).await
                                                    .and(tokio::fs::write(&out_path, &assembled).await)
                                                {
                                                    Ok(_) => {
                                                        let path_str = out_path.to_string_lossy().to_string();
                                                        let _ = self.storage.set_file_transfer_local_path(transfer_id, &path_str);
                                                        info!("✅ File saved: {}", path_str);
                                                        let message_id = self.storage.get_messages(&ft.chat_id, 200)
                                                            .unwrap_or_default()
                                                            .into_iter()
                                                            .find(|m| m.file_transfer_id.as_deref() == Some(transfer_id))
                                                            .map(|m| m.id)
                                                            .unwrap_or_else(|| transfer_id.clone());
                                                        let _ = self.event_tx.send(NetworkEvent::FileReceived {
                                                            peer_id: peer,
                                                            transfer_id: transfer_id.clone(),
                                                            message_id,
                                                            file_name: ft.file_name.clone(),
                                                            file_path: path_str,
                                                            size: ft.file_size,
                                                            mime_type: ft.mime_type.clone(),
                                                            caption: String::new(),
                                                        });
                                                    }
                                                    Err(e) => {
                                                        warn!("Failed to write file: {:?}", e);
                                                        let _ = self.storage.update_file_transfer_status(transfer_id, "failed");
                                                        let _ = self.event_tx.send(NetworkEvent::FileTransferFailed {
                                                            peer_id: peer,
                                                            transfer_id: transfer_id.clone(),
                                                            reason: e.to_string(),
                                                        });
                                                    }
                                                }
                                            }

                                            let _ = self.swarm.behaviour_mut().request_response
                                                .send_response(channel, Message::Pong);
                                        }

                                        Message::CallSignal { call_id, signal_json } => {
                                            let _ = self.event_tx.send(NetworkEvent::CallSignalReceived {
                                                peer_id: peer,
                                                call_id: call_id.clone(),
                                                signal_json: signal_json.clone(),
                                            });
                                            let _ = self.swarm.behaviour_mut().request_response.send_response(channel, Message::Pong);
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

                                request_response::Message::Response { request_id, response, .. } => {
                                    let pending = self.pending_requests.remove(&request_id);
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
                                        Message::Pong => {
                                            if let Some((_, Some(msg_id))) = &pending {
                                                let _ = self.storage.update_message_status(&msg_id, "delivered");
                                                let _ = self.event_tx.send(NetworkEvent::MessageDelivered {
                                                    peer_id: peer,
                                                    message_id: msg_id.clone(),
                                                });
                                            }

                                            if let Some((_, transfer_id, chunk_index)) = self.pending_file_requests.remove(&request_id) {
                                                if chunk_index == u32::MAX {
                                                    if let Some((_, chunks, _)) = self.pending_outgoing_chunks.get_mut(&transfer_id) {
                                                        if !chunks.is_empty() {
                                                            let chunk_data = chunks[0].clone();
                                                            let msg = Message::FileChunk {
                                                                transfer_id: transfer_id.clone(),
                                                                chunk_index: 0,
                                                                data: chunk_data,
                                                            };
                                                            let req_id = self.swarm.behaviour_mut().request_response.send_request(&peer, msg);

                                                            self.pending_file_requests.insert(req_id, (peer, transfer_id, 0));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Message::FileAck { transfer_id, chunk_index } => {
                                            let next_index = chunk_index + 1;
                                            if let Some((peer_id, chunks, _)) = self.pending_outgoing_chunks.get_mut(&transfer_id) {
                                                let total = chunks.len() as u32;
                                                let _ = self.storage.update_file_transfer_progress(&transfer_id, next_index);
                                                let _ = self.event_tx.send(NetworkEvent::FileTransferProgress {
                                                    peer_id: *peer_id,
                                                    transfer_id: transfer_id.clone(),
                                                    chunks_done: next_index,
                                                    total_chunks: total,
                                                });

                                                if next_index >= total {
                                                    let msg = Message::FileComplete { transfer_id: transfer_id.clone() };
                                                    let p = *peer_id;
                                                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&p, msg);
                                                    // u32::MAX signals FileComplete ACK — same as FileOffer ACK
                                                    self.pending_file_requests.insert(req_id, (p, transfer_id.clone(), u32::MAX));
                                                    // Mark completed in storage and notify UI
                                                    let _ = self.storage.update_file_transfer_status(&transfer_id, "completed");
                                                    let _ = self.event_tx.send(NetworkEvent::FileTransferCompleted {
                                                        peer_id: *peer_id,
                                                        transfer_id: transfer_id.clone(),
                                                    });
                                                    self.pending_outgoing_chunks.remove(&transfer_id);
                                                } else {
                                                    let chunk_data = chunks[next_index as usize].clone();
                                                    let p = *peer_id;
                                                    let tid = transfer_id.clone();
                                                    let msg = Message::FileChunk {
                                                        transfer_id: tid.clone(),
                                                        chunk_index: next_index,
                                                        data: chunk_data,
                                                    };
                                                    let req_id = self.swarm.behaviour_mut().request_response.send_request(&p, msg);
                                                    self.pending_file_requests.insert(req_id, (p, tid, next_index));
                                                }
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
                                    debug!("📊 Routing table updated for peer: {}", peer);
                                }
                                kad::Event::RoutablePeer { peer, address } => {
                                    info!("✅ Routable peer discovered: {} at {}", peer, address);
                                }
                                kad::Event::UnroutablePeer { peer } => {
                                    warn!("❌ Unroutable peer: {}", peer);
                                }
                                kad::Event::OutboundQueryProgressed { result, .. } => {
                                    match result {
                                        kad::QueryResult::Bootstrap(Ok(kad::BootstrapOk { peer, num_remaining })) => {
                                            info!("🚀 Bootstrap succeeded with peer {}, {} remaining", peer, num_remaining);

                                            if num_remaining == 0 {
                                                info!("✅ Bootstrap completed! DHT is ready.");
                                            }
                                        }
                                        kad::QueryResult::Bootstrap(Err(kad::BootstrapError::Timeout { .. })) => {
                                            warn!("⏱️ Bootstrap timeout - will retry");
                                        }
                                        kad::QueryResult::GetClosestPeers(Ok(kad::GetClosestPeersOk { peers, .. })) => {
                                            info!("🔍 Found {} closest peers", peers.len());
                                            
                                            for peer in peers {
                                                debug!("  - {:?}", peer);
                                            }
                                        }
                                        kad::QueryResult::GetClosestPeers(Err(e)) => {
                                            warn!("❌ GetClosestPeers failed: {:?}", e);
                                        }
                                        kad::QueryResult::GetRecord(Ok(kad::GetRecordOk::FoundRecord(record))) => {
                                            info!("📦 Found DHT record: {:?}", record.record.key);
                                        }
                                        kad::QueryResult::GetRecord(Err(e)) => {
                                            debug!("❌ GetRecord failed: {:?}", e);
                                        }
                                        kad::QueryResult::PutRecord(Ok(kad::PutRecordOk { key })) => {
                                            info!("✅ Successfully put record to DHT: {:?}", key);
                                        }
                                        kad::QueryResult::PutRecord(Err(e)) => {
                                            warn!("❌ PutRecord failed: {:?}", e);
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

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Autonat(event)) => {
                            match event {
                                autonat::Event::InboundProbe(e) => {
                                    debug!("🔍 Autonat inbound probe: {:?}", e);
                                }
                                autonat::Event::OutboundProbe(e) => {
                                    debug!("🔍 Autonat outbound probe: {:?}", e);
                                }
                                autonat::Event::StatusChanged { old, new } => {
                                    info!("🌐 NAT status changed: {:?} -> {:?}", old, new);
                                    match new {
                                        autonat::NatStatus::Public(addr) => {
                                            info!("✅ We are publicly reachable at: {:?}", addr);
                                        }
                                        autonat::NatStatus::Private => {
                                            info!("⚠️ We are behind NAT (private network)");
                                        }
                                        autonat::NatStatus::Unknown => {
                                            info!("❓ NAT status unknown");
                                        }
                                    }
                                }
                            }
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Identify(event)) => {
                            match event {
                                identify::Event::Received { peer_id, info, .. } => {
                                    info!("📝 Received identify info from {}", peer_id);
                                    debug!("  Protocol version: {}", info.protocol_version);
                                    debug!("  Agent version: {}", info.agent_version);
                                    debug!("  Protocols: {:?}", info.protocols);
                                    
                                    for addr in info.listen_addrs {
                                        self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                                    }
                                }
                                identify::Event::Sent { .. } => {
                                    debug!("📤 Sent identify info");
                                }
                                identify::Event::Pushed { .. } => {
                                    debug!("📤 Pushed identify info");
                                }
                                identify::Event::Error { peer_id, error, .. } => {
                                    warn!("❌ Identify error with {}: {:?}", peer_id, error);
                                }
                            }
                        }

                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }
}
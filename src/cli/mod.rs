use crate::network::{NetworkCommand, NetworkEvent};
use crate::storage::Storage;
use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info};

pub struct CLI {
    command_tx: mpsc::Sender<NetworkCommand>,
    event_rx: broadcast::Receiver<NetworkEvent>,
    storage: Arc<Storage>,
}

impl CLI {
    pub fn new(
        command_tx: mpsc::Sender<NetworkCommand>,
        event_rx: broadcast::Receiver<NetworkEvent>,
        storage: Arc<Storage>,
    ) -> Self {
        Self {
            command_tx,
            event_rx,
            storage,
        }
    }

    /// Форматирует peer_id с именем контакта, если оно есть
    fn format_peer(&self, peer_id: &PeerId) -> String {
        match self.storage.get_contact(peer_id) {
            Ok(Some(contact)) => {
                if let Some(name) = contact.name {
                    format!("{} ({})", name, peer_id)
                } else {
                    peer_id.to_string()
                }
            }
            _ => peer_id.to_string()
        }
    }

    /// Резолвит имя или peer_id в PeerId
    fn resolve_peer(&self, input: &str) -> Result<PeerId> {
        // Попробовать распарсить как PeerId
        match input.parse::<PeerId>() {
            Ok(pid) => Ok(pid),
            Err(_) => {
                // Если не PeerId, то искать по имени контакта
                match self.storage.find_contact_by_name(input) {
                    Ok(Some(contact)) => {
                        contact.peer_id.parse::<PeerId>()
                            .map_err(|_| anyhow::anyhow!("Invalid PeerId stored for contact: {}", input))
                    }
                    Ok(None) => {
                        Err(anyhow::anyhow!("Contact not found: {}. Use PeerId or add contact first.", input))
                    }
                    Err(e) => {
                        Err(anyhow::anyhow!("Failed to search contacts: {:?}", e))
                    }
                }
            }
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("\n VideoCalls CLI");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Commands:");
        println!("  peers                          - List connected peers");
        println!("  send <peer_id|name> <msg>      - Send message to peer");
        println!("  dial <multiaddr>               - Connect to peer");
        println!("  history <peer_id|name> [limit] - View chat history (default 20)");
        println!("  search <query> [limit]         - Search messages (default 50)");
        println!("  contacts                       - List all contacts");
        println!("  contact add <peer_id|name> [new_name]  - Add/update contact");
        println!("  contact remove <peer_id|name>  - Remove contact");
        println!("  channel subscribe <topic>      - Subscribe to channel");
        println!("  channel unsubscribe <topic>    - Unsubscribe from channel");
        println!("  channel publish <topic> <msg>  - Publish to channel");
        println!("  channel list                   - List subscribed channels");
        println!("  listen                         - Show listen addresses");
        println!("  identity                       - Show your PeerId");
        println!("  help                           - Show this help");
        println!("  quit                           - Exit");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin).lines();

        loop {
            tokio::select! {
                line = reader.next_line() => {
                    match line{
                        Ok(Some(line)) => {
                            if let Err(e) = self.handle_command(&line).await {
                                error!("Command error: {}", e);
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Failed to read line: {}", e);
                            break;
                        }
                    }
                }

                event = self.event_rx.recv() => {
                    match event {
                        Ok(event) => self.handle_event(event),
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            error!("Event receiver lagged by {} messages", n);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&self, line: &str) -> Result<()> {
        let parts: Vec<&str> = line.trim().split_whitespace().collect();

        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "help" => {
                println!("\nAvailable commands:");
                println!("  peers                          - List connected peers");
                println!("  send <peer_id|name> <msg>      - Send message to peer");
                println!("  dial <multiaddr>               - Connect to peer");
                println!("  history <peer_id|name> [limit] - View chat history (default 20)");
                println!("  search <query> [limit]         - Search messages (default 50)");
                println!("  contacts                       - List all contacts");
                println!("  contact add <peer_id|name> [new_name]  - Add/update contact");
                println!("  contact remove <peer_id|name>  - Remove contact");
                println!("  channel subscribe <topic>      - Subscribe to channel");
                println!("  channel unsubscribe <topic>    - Unsubscribe from channel");
                println!("  channel publish <topic> <msg>  - Publish to channel");
                println!("  channel list                   - List subscribed channels");
                println!("  listen                         - Show listen addresses");
                println!("  identity                       - Show your PeerId");
                println!("  bootstrap                      - Trigger DHT bootstrap");
                println!("  findpeer <peer_id>             - Find peer in DHT");
                println!("  dht                            - Show DHT routing table stats");
                println!("  nat                            - Show NAT status");
                println!("  quit                           - Exit\n");
            }

            "peers" => {
                self.command_tx
                    .send(NetworkCommand::GetPeers)
                    .await?;
            }

            "send" => {
                if parts.len() < 3 {
                    println!("Usage: send <peer_id|name> <message>");
                    return Ok(());
                }

                let peer_id = self.resolve_peer(parts[1])?;
                let message = parts[2..].join(" ");

                self.command_tx
                    .send(NetworkCommand::SendMessage { peer_id, message })
                    .await?;
            }

            "dial" => {
                if parts.len() < 2 {
                    println!("Usage: dial <multiaddr>");
                    return Ok(());
                }

                let address: Multiaddr = parts[1].parse()
                    .map_err(|_| anyhow::anyhow!("Invalid Multiaddr"))?;

                self.command_tx
                    .send(NetworkCommand::Dial { address })
                    .await?;
            }

            "listen" => {
                info!("Listening addresses are shown in the logs above");
            }

            "identity" => {
                info!("Your PeerId is shown in the logs above");
            }

            "quit" | "exit" => {
                self.command_tx
                    .send(NetworkCommand::Shutdown)
                    .await?;
                println!("👋 Goodbye!");
                std::process::exit(0);
            }

            "channel" => {
                if parts.len() < 2 {
                    println!("Usage:");
                    println!("  channel subscribe <topic>");
                    println!("  channel unsubscribe <topic>");
                    println!("  channel publish <topic> <message>");
                    println!("  channel list");
                    return Ok(());
                }

                match parts[1] {
                    "subscribe" => {
                        if parts.len() < 3 {
                            println!("Usage: channel subscribe <topic>");
                            return Ok(());
                        }
                        let topic = parts[2];
                        self.command_tx
                            .send(NetworkCommand::SubscribeChannel { topic: topic.to_string() })
                            .await?;
                    }
                    "unsubscribe" => {
                        if parts.len() < 3 {
                            println!("Usage: channel unsubscribe <topic>");
                            return Ok(());
                        }
                        let topic = parts[2];
                        self.command_tx
                            .send(NetworkCommand::UnsubscribeChannel { topic: topic.to_string() })
                            .await?;
                    }
                    "publish" => {
                        if parts.len() < 4 {
                            println!("Usage: channel publish <topic> <message>");
                            return Ok(());
                        }
                        let topic = parts[2];
                        let message = parts[3..].join(" ");
                        self.command_tx
                            .send(NetworkCommand::PublishToChannel {
                                topic: topic.to_string(),
                                message,
                            })
                            .await?;
                    }
                    "list" => {
                        self.command_tx
                            .send(NetworkCommand::ListChannels)
                            .await?;
                        }
                        _ => {
                            println!("Unknown channel command: {}", parts[1]);
                        }
                    }
                }

            "history" => {
                if parts.len() < 2 {
                    println!("Usage: history <peer_id|name> [limit]");
                    return Ok(());
                }

                let peer_id = self.resolve_peer(parts[1])?;

                let limit = if parts.len() >= 3 {
                    parts[2].parse().unwrap_or(20)
                } else {
                    20
                };

                let peer_display = self.format_peer(&peer_id);

                match self.storage.get_messages(&peer_id.to_string(), limit) {
                    Ok(messages) => {
                        if messages.is_empty() {
                            println!("\n📭 No messages found with {}\n", peer_display);
                        } else {
                            println!("\n📜 Chat history with {} (last {} messages):", peer_display, messages.len());
                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                            for msg in messages {
                                let timestamp = chrono::DateTime::from_timestamp(msg.timestamp, 0)
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "Unknown".to_string());

                                let direction = if msg.is_outgoing { "→" } else { "←" };

                                // Форматировать sender с именем
                                let sender_display = if let Ok(sender_peer) = msg.sender.parse::<PeerId>() {
                                    self.format_peer(&sender_peer)
                                } else {
                                    msg.sender.clone()
                                };

                                println!("[{}] {} {}: {}", timestamp, direction, sender_display, msg.content);
                            }

                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to get history: {:?}", e);
                    }
                }
            }

            "contacts" => {
                match self.storage.get_contacts() {
                    Ok(contacts) => {
                        if contacts.is_empty() {
                            println!("\n📭 No contacts found\n");
                        } else {
                            println!("\n👥 Contacts ({}):", contacts.len());
                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                            for contact in contacts {
                                let name = contact.name.as_deref().unwrap_or("Unknown");
                                let last_seen = contact.last_seen
                                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "Never".to_string());

                                println!("  {} | {} | Last seen: {}", contact.peer_id, name, last_seen);
                            }

                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to get contacts: {:?}", e);
                    }
                }
            }

            "contact" => {
                if parts.len() < 2 {
                    println!("Usage:");
                    println!("  contact add <peer_id> [name]");
                    println!("  contact remove <peer_id>");
                    return Ok(());
                }

                match parts[1] {
                    "add" => {
                        if parts.len() < 3 {
                            println!("Usage: contact add <peer_id|name> [new_name]");
                            return Ok(());
                        }

                        let peer_id = self.resolve_peer(parts[2])?;

                        let name = if parts.len() >= 4 {
                            Some(parts[3..].join(" "))
                        } else {
                            None
                        };

                        let contact = crate::storage::Contact {
                            peer_id: peer_id.to_string(),
                            name,
                            last_seen: Some(chrono::Utc::now().timestamp()),
                        };

                        match self.storage.save_contact(&contact) {
                            Ok(_) => {
                                let display = self.format_peer(&peer_id);
                                println!("✅ Contact saved: {}", display);
                            }
                            Err(e) => {
                                println!("❌ Failed to save contact: {:?}", e);
                            }
                        }
                    }
                    "remove" => {
                        if parts.len() < 3 {
                            println!("Usage: contact remove <peer_id|name>");
                            return Ok(());
                        }

                        let peer_id = self.resolve_peer(parts[2])?;

                        match self.storage.remove_contact(&peer_id) {
                            Ok(_) => {
                                println!("✅ Contact removed: {}", peer_id);
                            }
                            Err(e) => {
                                println!("❌ Failed to remove contact: {:?}", e);
                            }
                        }
                    }
                    _ => {
                        println!("Unknown contact command: {}", parts[1]);
                    }
                }
            }
            "search" => {
                if parts.len() < 2 {
                    println!("Usage: search <query> [limit]");
                    return Ok(());
                }

                let query = parts[1];
                let limit = if parts.len() >= 3 {
                    parts[2].parse().unwrap_or(50)
                } else {
                    50
                };

                match self.storage.search_messages(query, limit) {
                    Ok(messages) => {
                        if messages.is_empty() {
                            println!("\n📭 No messages found for query: '{}'\n", query);
                        } else {
                            println!("\n🔍 Search results for '{}' ({} messages):", query, messages.len());
                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                            for msg in messages {
                                let timestamp = chrono::DateTime::from_timestamp(msg.timestamp, 0)
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                                    .unwrap_or_else(|| "Unknown".to_string());

                                let direction = if msg.is_outgoing { "→" } else { "←" };

                                // Форматировать chat_id и sender с именами
                                let chat_display = if let Ok(chat_peer) = msg.chat_id.parse::<PeerId>() {
                                    self.format_peer(&chat_peer)
                                } else {
                                    msg.chat_id.clone()
                                };

                                let sender_display = if let Ok(sender_peer) = msg.sender.parse::<PeerId>() {
                                    self.format_peer(&sender_peer)
                                } else {
                                    msg.sender.clone()
                                };

                                println!("[{}] {} {} | {}: {}",
                                    timestamp, direction, chat_display, sender_display, msg.content);
                            }

                            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
                        }
                    }
                    Err(e) => {
                        println!("❌ Failed to search messages: {:?}", e);
                    }
                }
            }
            "bootstrap" => {
                println!("🚀 Triggering DHT bootstrap...");
                self.command_tx
                    .send(NetworkCommand::Bootstrap)
                    .await?;
            }
            "findpeer" => {
                if parts.len() < 2 {
                    println!("Usage: findpeer <peer_id>");
                    return Ok(());
                }

                let peer_id: PeerId = parts[1].parse()
                    .map_err(|_| anyhow::anyhow!("Invalid PeerId"))?;

                println!("🔍 Searching for peer {} in DHT...", peer_id);
                self.command_tx
                    .send(NetworkCommand::FindPeer { peer_id })
                    .await?;
            }
            "dht" => {
                println!("📊 Fetching DHT routing table stats...");
                self.command_tx
                    .send(NetworkCommand::GetDhtStats)
                    .await?;
            }
            "nat" => {
                println!("🌐 Checking NAT status...");
                self.command_tx
                    .send(NetworkCommand::GetNatStatus)
                    .await?;
            }
            _ => {
                println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
            }
        }

        Ok(())
    }

    fn handle_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived { peer_id, message } => {
                let peer_display = self.format_peer(&peer_id);
                println!("\n💬 Message from {}: {}\n", peer_display, message);
            }
            NetworkEvent::PeerConnected { peer_id } => {
                let peer_display = self.format_peer(&peer_id);
                println!("\n✅ Peer connected: {}\n", peer_display);
            }
            NetworkEvent::PeerDisconnected { peer_id } => {
                let peer_display = self.format_peer(&peer_id);
                println!("\n❌ Peer disconnected: {}\n", peer_display);
            }
            NetworkEvent::PeerDiscovered { peer_id, address } => {
                let peer_display = self.format_peer(&peer_id);
                println!("\n🔍 Peer discovered: {} at {}\n", peer_display, address);
            }
            NetworkEvent::ListenAddrAdded { address } => {
                println!("\n📡 Listening on: {}\n", address);
            }
            NetworkEvent::ChannelMessage { topic, peer_id, message } => {
                let peer_display = self.format_peer(&peer_id);
                println!("\n📢 Channel [{}] from {}: {}\n", topic, peer_display, message);
            }
        }
    }
}
use crate::network::{NetworkCommand, NetworkEvent};
use anyhow::Result;
use libp2p::{Multiaddr, PeerId};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info};

pub struct CLI {
    command_tx: mpsc::Sender<NetworkCommand>,
    event_rx: broadcast::Receiver<NetworkEvent>,
}

impl CLI {
    pub fn new(
        command_tx: mpsc::Sender<NetworkCommand>,
        event_rx: broadcast::Receiver<NetworkEvent>,
    ) -> Self {
        Self {
            command_tx,
            event_rx,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("\n VideoCalls CLI");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Commands:");
        println!("  peers                         - List connected peers");
        println!("  send <peer_id> <msg>          - Send message to peer");
        println!("  dial <multiaddr>              - Connect to peer");
        println!("  channel subscribe <topic>     - Subscribe to channel");
        println!("  channel unsubscribe <topic>   - Unsubscribe from channel");
        println!("  channel publish <topic> <msg> - Publish to channel");
        println!("  channel list                  - List subscribed channels");
        println!("  listen                        - Show listen addresses");
        println!("  identity                      - Show your PeerId");
        println!("  help                          - Show this help");
        println!("  quit                          - Exit");
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
                println!("  peers                         - List connected peers");
                println!("  send <peer_id> <msg>          - Send message to peer");
                println!("  dial <multiaddr>              - Connect to peer");
                println!("  channel subscribe <topic>     - Subscribe to channel");
                println!("  channel unsubscribe <topic>   - Unsubscribe from channel");
                println!("  channel publish <topic> <msg> - Publish to channel");
                println!("  channel list                  - List subscribed channels");
                println!("  listen                        - Show listen addresses");
                println!("  identity                      - Show your PeerId");
                println!("  quit                          - Exit\n");
            }

            "peers" => {
                self.command_tx
                    .send(NetworkCommand::GetPeers)
                    .await?;
            }

            "send" => {
                if parts.len() < 3 {
                    println!("Usage: send <peer_id> <message>");
                    return Ok(());
                }

                let peer_id: PeerId = parts[1].parse()
                    .map_err(|_| anyhow::anyhow!("Invalid PeerId"))?;

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

            _ => {
                println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
            }
        }

        Ok(())
    }

    fn handle_event(&self, event: NetworkEvent) {
        match event {
            NetworkEvent::MessageReceived { peer_id, message } => {
                println!("\n💬 Message from {}: {}\n", peer_id, message);
            }
            NetworkEvent::PeerConnected { peer_id } => {
                println!("\n✅ Peer connected: {}\n", peer_id);
            }
            NetworkEvent::PeerDisconnected { peer_id } => {
                println!("\n❌ Peer disconnected: {}\n", peer_id);
            }
            NetworkEvent::PeerDiscovered { peer_id, address } => {
                println!("\n🔍 Peer discovered: {} at {}\n", peer_id, address);
            }
            NetworkEvent::ListenAddrAdded { address } => {
                println!("\n📡 Listening on: {}\n", address);
            }
            NetworkEvent::ChannelMessage { topic, peer_id, message } => {
                println!("\n📢 Channel [{}] from {}: {}\n", topic, peer_id, message);
            }
        }
    }
}
pub mod behaviour;
pub mod transport;

use crate::identity::Identity;
use anyhow::Result;
use libp2p::{Multiaddr, PeerId, Swarm};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub enum NetworkCommand {
    SendMessage { peer_id: PeerId, message: String },
    Dial { address: Multiaddr },
    GetPeers,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerDiscovered { peer_id: PeerId, address: Multiaddr },
    PeerConnected { peer_id: PeerId },
    PeerDisconnected { peer_id: PeerId },
    MessageReceived { peer_id: PeerId, message: String },
    ListenAddrAdded { address: Multiaddr },
}

pub struct P2PNetwork {
    swarm: Swarm<behaviour::MyBehaviour>,
    command_rx: mpsc::Receiver<NetworkCommand>,
    command_tx: mpsc::Sender<NetworkCommand>,
    event_tx: broadcast::Sender<NetworkEvent>,
}

impl P2PNetwork {
    pub fn new(identity: &Identity) -> Result<Self> {
        let keypair = identity.keypair().clone();
        let peer_id = *identity.peer_id();

        let transport = transport::build_transport(&keypair)?;

        let behaviour = behaviour::MyBehaviour::new(peer_id)?;

        let swarm = Swarm::new(
            transport,
            behaviour,
            peer_id,
            libp2p::swarm::Config::with_tokio_executor()
                .with_idle_connection_timeout(std::time::Duration::from_secs(60)),
        );

        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, _) = broadcast::channel(100);

        Ok(Self {
            swarm,
            command_rx,
            command_tx,
            event_tx,
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

        info!("Network started");

        loop {
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
                    match command {
                        NetworkCommand::Shutdown => {
                            info!("Shutting down network");
                            break;
                        }
                        NetworkCommand::Dial { address } => {
                            debug!("Dialing {}", address);
                            if let Err(e) = self.swarm.dial(address.clone()) {
                                warn!("Failed to dial {}: {:?}", address, e);
                            }
                        }
                        NetworkCommand::GetPeers => {
                            let peers: Vec<_> = self.swarm.connected_peers().collect();
                            info!("Connected peers: {:?}", peers);
                        }
                        NetworkCommand::SendMessage { peer_id, message } => {
                            info!("Sending message to {}: {}", peer_id, message);
                        }
                    }
                }

                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Listening on: {}", address);
                            let _ = self.event_tx.send(NetworkEvent::ListenAddrAdded { address });
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                            for (peer_id, addr) in peers {
                                info!("Discovered peer: {} at {}", peer_id, addr);
                                let _ = self.event_tx.send(NetworkEvent::PeerDiscovered { peer_id, address: addr.clone() });
                                if let Err(e) = self.swarm.dial(addr) {
                                    warn!("Failed to dial peer: {:?}", e);
                                }
                            }
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                            for (peer_id, _) in peers {
                                info!("Peer expired: {}", peer_id);
                            }
                        }

                        SwarmEvent::ConnectionEstablished { peer_id, endpoint, ..} => {
                            info!("Connected to {} at {}", peer_id, endpoint.get_remote_address());
                            let _ = self.event_tx.send(NetworkEvent::PeerConnected { peer_id });
                        }

                        SwarmEvent::ConnectionClosed { peer_id, cause, ..} => {
                            info!("Disconnected from {} (cause {:?})", peer_id, cause);
                            let _ = self.event_tx.send(NetworkEvent::PeerDisconnected { peer_id });
                        }

                        SwarmEvent::Behaviour(behaviour::MyBehaviourEvent::Ping(event)) => {
                            debug!("Ping event: {:?}", event);
                        }

                        SwarmEvent::IncomingConnection { send_back_addr, .. } => {
                            debug!("Incoming connection from {}", send_back_addr);
                        }

                        _ => {}
                    }
                }
            }
        }

        Ok(())
    }
}
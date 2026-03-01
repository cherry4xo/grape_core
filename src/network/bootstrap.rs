use libp2p::{Multiaddr, PeerId};

pub struct BootstrapNode {
    pub peer_id: PeerId,
    pub address: Multiaddr,
}

pub fn get_bootstrap_nodes() -> Vec<BootstrapNode> {
    vec![]
}

pub fn setup_bootstrap(behaviour: &mut crate::network::behaviour::MyBehaviour) {
    for node in get_bootstrap_nodes() {
        behaviour.add_bootstrap_peer(node.peer_id, node.address);
    }

    if let Err(e) = behaviour.bootstrap() {
        tracing::warn!("Failed to start bootstrap: {:?}", e);
    }
}
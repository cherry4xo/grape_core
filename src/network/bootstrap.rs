use std::str::FromStr;

use libp2p::{Multiaddr, PeerId};
use tracing::{info, warn};

pub struct BootstrapNode {
    pub peer_id: PeerId,
    pub address: Multiaddr,
}

pub fn get_bootstrap_nodes() -> Vec<BootstrapNode> {
    vec![
        // 1. Rust-based node (ny5) - Рекомендуется для Rust-проектов
        // Официальный узел IPFS Foundation, расположен в NY [1]
        BootstrapNode {
            peer_id: PeerId::from_str("QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa").unwrap(),
            address: "/dnsaddr/ny5.bootstrap.libp2p.io".parse().unwrap(),
        },

        // 2. JS-libp2p node (va1) - Новый узел (2024)
        // Добавлен для разнообразия реализаций в сети [1]
        BootstrapNode {
            peer_id: PeerId::from_str("12D3KooWKnDdG3iXw9eTFijk3EWSunZcFi54Zka4wmtqtt6rPxc8").unwrap(),
            address: "/dnsaddr/va1.bootstrap.libp2p.io".parse().unwrap(),
        },

        // 3. Стандартный Go-узел (sv15)
        // Ранее известный вам как QmNnooDu...
        BootstrapNode {
            peer_id: PeerId::from_str("QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN").unwrap(),
            address: "/dnsaddr/sv15.bootstrap.libp2p.io".parse().unwrap(),
        },

        // 4. Стандартный Go-узел (am6)
        // Ранее известный вам как QmbLHAnM...
        BootstrapNode {
            peer_id: PeerId::from_str("QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb").unwrap(),
            address: "/dnsaddr/am6.bootstrap.libp2p.io".parse().unwrap(),
        },

        // 5. Стандартный Go-узел (sg1)
        // Ранее известный вам как QmcZf59b...
        BootstrapNode {
            peer_id: PeerId::from_str("QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt").unwrap(),
            address: "/dnsaddr/sg1.bootstrap.libp2p.io".parse().unwrap(),
        },

        // Legacy/Direct IP Fallback (opional)
        // Узел mars.i.ipfs.io (оставляем прямой IP для подстраховки, если DNS недоступен)
        BootstrapNode {
            peer_id: PeerId::from_str("QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ").unwrap(),
            address: "/ip4/104.131.131.82/tcp/4001/p2p/QmaCpDMGvV2BGHeYERUEnRQAwe3N8SzbUtfsmvsqQLuvuJ".parse().unwrap(),
        },
    ]
}

pub fn setup_bootstrap(behaviour: &mut crate::network::behaviour::MyBehaviour) {
    for node in get_bootstrap_nodes() {
        info!("Adding bootstrap node: {} at {}", node.peer_id, node.address);
        behaviour.add_bootstrap_peer(node.peer_id, node.address);
    }

    match behaviour.bootstrap() {
        Ok(query_id) => {
            info!("✅ Bootstrap started with query_id: {:?}", query_id);
        }
        Err(e) => {
            warn!("❌ Failed to start bootstrap: {:?}", e);
        }
    }
}
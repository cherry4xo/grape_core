use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed, upgrade},
    identity::Keypair,
    noise, tcp, yamux, Transport,
};
use anyhow::Result;

pub fn build_transport(keypair: &Keypair) -> Result<Boxed<(libp2p::PeerId, StreamMuxerBox)>> {
    let transport = tcp::tokio::Transport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(keypair)?)
        .multiplex(yamux::Config::default())
        .boxed();

    Ok(transport)
}
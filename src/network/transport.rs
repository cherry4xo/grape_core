use libp2p::{
    core::{muxing::StreamMuxerBox, transport::Boxed, upgrade, Transport},
    identity::Keypair,
    noise, tcp, yamux, quic, dns
};
use anyhow::Result;
use futures::future::Either;

pub fn build_transport(
    keypair: &Keypair,
) -> Result<Boxed<(libp2p::PeerId, StreamMuxerBox)>> {
    let tcp_transport = tcp::tokio::Transport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(noise::Config::new(keypair)?)
        .multiplex(yamux::Config::default())
        .map(|(peer_id, muxer), _| (peer_id, StreamMuxerBox::new(muxer)));

    let quic_transport = quic::tokio::Transport::new(quic::Config::new(keypair))
        .map(|(peer_id, muxer), _| (peer_id, StreamMuxerBox::new(muxer)));

    let transport = quic_transport
        .or_transport(tcp_transport)
        .map(|either, _| match either {
            Either::Left(output) => output,
            Either::Right(output) => output,
        });

    // Wrap with DNS resolver to support /dnsaddr/, /dns4/, /dns6/ multiaddrs
    let transport = dns::tokio::Transport::system(transport)?
        .boxed();

    Ok(transport)
}
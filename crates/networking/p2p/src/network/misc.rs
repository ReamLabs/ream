use std::{io, pin::Pin, time::Duration};

use libp2p::{
    Transport,
    core::{
        muxing::StreamMuxerBox,
        transport::Boxed,
        upgrade::{SelectUpgrade, Version},
    },
    dns::Transport as DnsTransport,
    noise::Config as NoiseConfig,
    tcp::{Config as TcpConfig, tokio::Transport as TcpTransport},
    yamux,
};
use libp2p_identity::{Keypair, PeerId};
use libp2p_mplex::{MaxBufferBehaviour, MplexConfig};
use ream_executor::ReamExecutor;
use yamux::Config as YamuxConfig;

pub struct Executor(pub ReamExecutor);

impl libp2p::swarm::Executor for Executor {
    fn exec(&self, f: Pin<Box<dyn futures::Future<Output = ()> + Send>>) {
        self.0.spawn(f);
    }
}

pub fn build_transport(local_private_key: Keypair) -> io::Result<Boxed<(PeerId, StreamMuxerBox)>> {
    // mplex config
    let mut mplex_config = MplexConfig::new();
    mplex_config.set_max_buffer_size(256);
    mplex_config.set_max_buffer_behaviour(MaxBufferBehaviour::Block);

    let yamux_config = YamuxConfig::default();

    let tcp = TcpTransport::new(TcpConfig::default().nodelay(true))
        .upgrade(Version::V1)
        .authenticate(NoiseConfig::new(&local_private_key).expect("Noise disabled"))
        .multiplex(SelectUpgrade::new(yamux_config, mplex_config))
        .timeout(Duration::from_secs(10));
    let transport = tcp.boxed();

    let transport = DnsTransport::system(transport)?.boxed();

    Ok(transport)
}

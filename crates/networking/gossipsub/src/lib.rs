pub mod config;
pub mod handler;
pub mod protocol;
pub mod snappy;
pub mod topics;

use config::GossipsubConfig;
use handler::GossipsubConnectionHandler;
use libp2p::swarm::NetworkBehaviour;
use snappy::SnappyTransform;
use topics::GossipTopic;

pub struct Gossipsub {
    pub config: GossipsubConfig,
    pub snappy_transform: SnappyTransform,
}

impl Gossipsub {
    pub fn new(config: GossipsubConfig, snappy_transform: SnappyTransform) -> Self {
        Self {
            config,
            snappy_transform,
        }
    }

    pub fn subscribe(&mut self, _topic: GossipTopic) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn unsubscribe(&mut self, _topic: GossipTopic) -> anyhow::Result<bool> {
        todo!()
    }
}

impl NetworkBehaviour for Gossipsub {
    type ConnectionHandler = GossipsubConnectionHandler;
    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: libp2p::PeerId,
        _local_addr: &libp2p::Multiaddr,
        _remote_addr: &libp2p::Multiaddr,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        todo!()
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: libp2p::swarm::ConnectionId,
        _peer: libp2p::PeerId,
        _addr: &libp2p::Multiaddr,
        _role_override: libp2p::core::Endpoint,
        _port_use: libp2p::core::transport::PortUse,
    ) -> Result<libp2p::swarm::THandler<Self>, libp2p::swarm::ConnectionDenied> {
        todo!()
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: libp2p::PeerId,
        _connection_id: libp2p::swarm::ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
        todo!()
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<libp2p::swarm::ToSwarm<Self::ToSwarm, libp2p::swarm::THandlerInEvent<Self>>>
    {
        todo!()
    }

    fn on_swarm_event(&mut self, _event: libp2p::swarm::FromSwarm) {
        todo!()
    }
}

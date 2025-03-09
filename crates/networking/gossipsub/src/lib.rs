pub mod config;
pub mod handler;
pub mod protocol;
pub mod snappy;
pub mod topics;
pub mod types;

use std::collections::{HashMap, HashSet};

use config::GossipsubConfig;
use handler::GossipsubConnectionHandler;
use libp2p::{
    gossipsub::{Message, MessageId},
    swarm::NetworkBehaviour,
    PeerId,
};
use snappy::SnappyTransform;
use topics::{GossipTopic, TopicName};
use types::ConnectionInfo;

pub struct Gossipsub {
    pub config: GossipsubConfig,
    pub snappy_transform: SnappyTransform,
    pub connected_peers: HashMap<PeerId, ConnectionInfo>,
    pub peers_by_topic: HashMap<TopicName, HashSet<PeerId>>,
}

impl Gossipsub {
    pub fn new(config: GossipsubConfig, snappy_transform: SnappyTransform) -> Self {
        Self {
            config,
            snappy_transform,
            connected_peers: HashMap::new(),
            peers_by_topic: HashMap::new(),
        }
    }

    pub fn subscribe(&mut self, _topic: GossipTopic) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn unsubscribe(&mut self, _topic: GossipTopic) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn publish(&mut self, _topic: GossipTopic, _data: Vec<u8>) -> anyhow::Result<MessageId> {
        todo!()
    }
}

#[derive(Debug)]
pub enum GossipsubEvent {
    Message {
        peer_id: PeerId,
        message_id: MessageId,
        message: Message,
    },
    Subscribed {
        peer_id: PeerId,
        topic: TopicName,
    },
    Unsubscribed {
        peer_id: PeerId,
        topic: TopicName,
    },
}

impl NetworkBehaviour for Gossipsub {
    type ConnectionHandler = GossipsubConnectionHandler;
    type ToSwarm = GossipsubEvent;

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

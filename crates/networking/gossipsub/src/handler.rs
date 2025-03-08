use libp2p::swarm::ConnectionHandler;

use super::protocol::GossipsubProtocol;

#[derive(Debug)]
pub struct GossipsubInMessage {}

#[derive(Debug)]
pub struct GossipsubOutMessage {}

pub struct GossipsubConnectionHandler {}

impl ConnectionHandler for GossipsubConnectionHandler {
    type FromBehaviour = GossipsubInMessage;
    type ToBehaviour = GossipsubOutMessage;
    type InboundOpenInfo = ();
    type InboundProtocol = GossipsubProtocol;
    type OutboundOpenInfo = ();
    type OutboundProtocol = GossipsubProtocol;

    fn listen_protocol(
        &self,
    ) -> libp2p::swarm::SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        todo!()
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {
        todo!()
    }

    fn connection_keep_alive(&self) -> bool {
        todo!()
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<
        libp2p::swarm::ConnectionHandlerEvent<
            Self::OutboundProtocol,
            Self::OutboundOpenInfo,
            Self::ToBehaviour,
        >,
    > {
        todo!()
    }

    fn on_connection_event(
        &mut self,
        _event: libp2p::swarm::handler::ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        todo!()
    }
}

use libp2p::swarm::{
    handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound},
    ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol,
};
use tracing::warn;

use super::{inbound_protocol::ReqRespInboundProtocol, outbound_protocol::ReqRespOutboundProtocol};

#[derive(Debug)]
pub struct OutboundReqRespMessage {}

#[derive(Debug)]
pub struct InboundReqRespMessage {}

pub struct ReqRespConnectionHandler {
    listen_protocol: SubstreamProtocol<ReqRespInboundProtocol, ()>,
}

impl ReqRespConnectionHandler {
    pub fn new(listen_protocol: SubstreamProtocol<ReqRespInboundProtocol, ()>) -> Self {
        ReqRespConnectionHandler { listen_protocol }
    }
}

impl ConnectionHandler for ReqRespConnectionHandler {
    type FromBehaviour = OutboundReqRespMessage;

    type ToBehaviour = InboundReqRespMessage;

    type InboundProtocol = ReqRespInboundProtocol;

    type OutboundProtocol = ReqRespOutboundProtocol;

    type InboundOpenInfo = ();

    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn poll(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        warn!("ReqRespConnectionHandler poll");
        std::task::Poll::Pending
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {
        warn!("Unexpected behaviour event: {:?}", _event);
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        warn!("Unexpected connection event: {:?}", event);
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound { .. }) => todo!(),
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound { .. }) => todo!(),
            ConnectionEvent::DialUpgradeError(_) => todo!(),
            _ => {
                // ConnectionEvent is not exhaustive so we have to account for the default case
            }
        }
    }

    fn connection_keep_alive(&self) -> bool {
        false
    }

    fn poll_close(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::ToBehaviour>> {
        std::task::Poll::Ready(None)
    }
}

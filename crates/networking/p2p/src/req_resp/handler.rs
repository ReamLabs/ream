use std::task::{Context, Poll};

use libp2p::swarm::{
    handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound},
    ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol,
};
use tracing::warn;

use super::{
    error::ReqRespError, inbound_protocol::InboundReqRespProtocol,
    outbound_protocol::OutboundReqRespProtocol,
};

#[derive(Debug)]
pub enum ConnectionRequest {
    Request(()),
    Response(()),
    Shutdown,
}

#[derive(Debug)]
pub enum HandlerEvent {
    Ok(()),
    Err(ReqRespError),
    Close,
}

pub struct ReqRespConnectionHandler {
    listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>,
}

impl ReqRespConnectionHandler {
    pub fn new(listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>) -> Self {
        ReqRespConnectionHandler { listen_protocol }
    }
}

impl ConnectionHandler for ReqRespConnectionHandler {
    type FromBehaviour = ConnectionRequest;
    type ToBehaviour = HandlerEvent;
    type InboundProtocol = InboundReqRespProtocol;
    type OutboundProtocol = OutboundReqRespProtocol;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        self.listen_protocol.clone()
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        warn!("ReqRespConnectionHandler poll");
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: ConnectionRequest) {
        warn!("Unexpected behaviour event: {:?}", event);
        match event {
            ConnectionRequest::Request(_) => todo!(),
            ConnectionRequest::Response(_) => todo!(),
            ConnectionRequest::Shutdown => todo!(),
        }
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

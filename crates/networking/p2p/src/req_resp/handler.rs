use std::task::{Context, Poll};

use libp2p::{
    swarm::{
        handler::{
            ConnectionEvent, DialUpgradeError, FullyNegotiatedInbound, FullyNegotiatedOutbound,
        },
        ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError, SubstreamProtocol,
    },
    Stream,
};
use tracing::{info, warn};

use crate::req_resp::ConnectionRequest;

use super::{
    error::ReqRespError,
    inbound_protocol::{InboundOutput, InboundReqRespProtocol},
    messages::Messages,
    outbound_protocol::{OutboundFramed, OutboundReqRespProtocol},
};

#[derive(Debug)]
pub enum HandlerEvent {
    Ok(()),
    Err(ReqRespError),
    Close,
}

pub struct ReqRespConnectionHandler {
    listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>,
    _request_queue: Vec<Messages>,
}

impl ReqRespConnectionHandler {
    pub fn new(listen_protocol: SubstreamProtocol<InboundReqRespProtocol, ()>) -> Self {
        ReqRespConnectionHandler {
            listen_protocol,
            _request_queue: vec![],
        }
    }

    fn on_fully_negotiated_inbound(&mut self, inbound_output: InboundOutput<Stream>, _info: ()) {}

    fn on_fully_negotiated_outbound(&mut self, outbound_output: OutboundFramed<Stream>, _info: ()) {
    }

    fn on_dial_upgrade_error(&mut self, error: StreamUpgradeError<ReqRespError>, _info: ()) {}
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
            ConnectionRequest::Request(request) => info!(?request, "Request"),
            ConnectionRequest::Response(response) => info!(?response, "Response"),
            ConnectionRequest::Shutdown => info!("Shutdown"),
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
        warn!("On connection event: {:?}", event);
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound { protocol, info }) => {
                self.on_fully_negotiated_inbound(protocol, info)
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol,
                info,
            }) => {
                self.on_fully_negotiated_outbound(protocol, info);
            }
            ConnectionEvent::DialUpgradeError(DialUpgradeError { error, info }) => {
                self.on_dial_upgrade_error(error, info);
            }
            // ConnectionEvent is not exhaustive so we have to account for the default case
            _ => (),
        }
    }

    fn connection_keep_alive(&self) -> bool {
        false
    }
}

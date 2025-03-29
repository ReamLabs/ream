pub mod configurations;
pub mod error;
pub mod handler;
pub mod inbound_protocol;
pub mod messages;
pub mod outbound_protocol;
pub mod protocol_id;
pub mod utils;

use std::task::{Context, Poll};

use error::ReqRespError;
use handler::{ConnectionRequest, HandlerEvent, ReqRespConnectionHandler};
use inbound_protocol::InboundReqRespProtocol;
use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        ConnectionDenied, ConnectionHandler, ConnectionId, FromSwarm, NetworkBehaviour,
        SubstreamProtocol, THandler, THandlerInEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};

/// Maximum number of concurrent requests per protocol ID that a client may issue.
pub const MAX_CONCURRENT_REQUESTS: usize = 2;

#[derive(Debug)]
pub struct ReqRespMessage {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub message: Result<(), ReqRespError>,
}

pub struct ReqResp {
    pub events: Vec<ToSwarm<ReqRespMessage, ConnectionRequest>>,
}

impl ReqResp {
    pub fn new() -> Self {
        ReqResp { events: vec![] }
    }
}

impl Default for ReqResp {
    fn default() -> Self {
        ReqResp::new()
    }
}

impl NetworkBehaviour for ReqResp {
    type ConnectionHandler = ReqRespConnectionHandler;

    type ToSwarm = ReqRespMessage;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let listen_protocol = SubstreamProtocol::new(InboundReqRespProtocol {}, ());

        Ok(ReqRespConnectionHandler::new(listen_protocol))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let listen_protocol = SubstreamProtocol::new(InboundReqRespProtocol {}, ());

        Ok(ReqRespConnectionHandler::new(listen_protocol))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {
        // Nothing that is relevant to us currently.
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        match event {
            HandlerEvent::Ok(_) => todo!(),
            HandlerEvent::Err(err) => self.events.push(ToSwarm::GenerateEvent(ReqRespMessage {
                peer_id,
                connection_id,
                message: Err(err),
            })),
            HandlerEvent::Close => todo!(),
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if !self.events.is_empty() {
            return Poll::Ready(self.events.remove(0));
        }

        Poll::Pending
    }

    fn handle_pending_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        Ok(())
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        Ok(std::vec![])
    }
}

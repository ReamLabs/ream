pub mod error;
pub mod handler;
pub mod inbound_protocol;
pub mod outbound_protocol;
pub mod protocol_id;

use std::task::{Context, Poll};

use handler::ReqRespConnectionHandler;
use inbound_protocol::ReqRespInboundProtocol;
use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, SubstreamProtocol, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};

/// Maximum number of concurrent requests per protocol ID that a client may issue.
pub const MAX_CONCURRENT_REQUESTS: usize = 2;

pub struct ReqResp {}

impl ReqResp {
    pub fn new() -> Self {
        ReqResp {}
    }
}

impl Default for ReqResp {
    fn default() -> Self {
        ReqResp::new()
    }
}

impl NetworkBehaviour for ReqResp {
    type ConnectionHandler = ReqRespConnectionHandler;

    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let listen_protocol = SubstreamProtocol::new(ReqRespInboundProtocol {}, ());

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
        let listen_protocol = SubstreamProtocol::new(ReqRespInboundProtocol {}, ());

        Ok(ReqRespConnectionHandler::new(listen_protocol))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {
        // Nothing that is relevant to us currently.
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        tracing::error!("ReqResp poll");
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

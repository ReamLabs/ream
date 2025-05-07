pub mod configurations;
pub mod error;
pub mod handler;
pub mod inbound_protocol;
pub mod messages;
pub mod outbound_protocol;
pub mod protocol_id;

use std::task::{Context, Poll};

use error::ReqRespError;
use handler::{HandlerEvent, ReqRespConnectionHandler, ReqRespMessageReceived, RespMessage};
use inbound_protocol::InboundReqRespProtocol;
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    swarm::{
        CloseConnection, ConnectionDenied, ConnectionHandler, ConnectionId, FromSwarm,
        NetworkBehaviour, NotifyHandler, SubstreamProtocol, THandler, THandlerInEvent, ToSwarm,
    },
};
use messages::Message;
use tracing::{debug, info};

/// Maximum number of concurrent requests per protocol ID that a client may issue.
pub const MAX_CONCURRENT_REQUESTS: usize = 2;

#[derive(Debug)]
pub struct ReqRespMessage {
    pub peer_id: PeerId,
    pub connection_id: ConnectionId,
    pub message: Result<ReqRespMessageReceived, ReqRespError>,
}

#[derive(Debug)]
pub enum ConnectionRequest {
    Request {
        request_id: u64,
        message: Message,
    },
    Response {
        stream_id: u64,
        message: RespMessage,
    },
    Shutdown,
}

pub struct ReqResp {
    pub events: Vec<ToSwarm<ReqRespMessage, ConnectionRequest>>,
}

impl ReqResp {
    pub fn new() -> Self {
        ReqResp { events: vec![] }
    }

    pub fn send_request(&mut self, peer_id: PeerId, request_id: u64, message: Message) {
        self.events.push(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::Any,
            event: ConnectionRequest::Request {
                request_id,
                message,
            },
        });
    }

    pub fn send_response(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        stream_id: u64,
        message: RespMessage,
    ) {
        self.events.push(ToSwarm::NotifyHandler {
            peer_id,
            handler: NotifyHandler::One(connection_id),
            event: ConnectionRequest::Response { stream_id, message },
        });
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
        connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        debug!(
            "REQRESP: Handling established inbound connection {connection_id:?} {peer:?} {_remote_addr:?}",
        );
        let listen_protocol = SubstreamProtocol::new(InboundReqRespProtocol {}, ());

        Ok(ReqRespConnectionHandler::new(listen_protocol))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        debug!(
            "REQRESP: Handling established outbound connection {connection_id:?} {peer:?} {_addr:?}",
        );
        let listen_protocol = SubstreamProtocol::new(InboundReqRespProtocol {}, ());
        Ok(ReqRespConnectionHandler::new(listen_protocol))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {
        // Nothing that is relevant to us currently.
        info!("REQRESP: Handling swarm event {:?}", _event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: <Self::ConnectionHandler as ConnectionHandler>::ToBehaviour,
    ) {
        info!(
            "REQRESP: Handling connection handler event {:?} {:?} {:?}",
            peer_id, connection_id, event
        );
        match event {
            HandlerEvent::Ok(message) => self.events.push(ToSwarm::GenerateEvent(ReqRespMessage {
                peer_id,
                connection_id,
                message: Ok(message),
            })),
            HandlerEvent::Err(err) => self.events.push(ToSwarm::GenerateEvent(ReqRespMessage {
                peer_id,
                connection_id,
                message: Err(err),
            })),
            HandlerEvent::Close => self.events.push(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            }),
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        debug!("REQRESP: Polling events {:?}", self.events);
        if !self.events.is_empty() {
            return Poll::Ready(self.events.remove(0));
        }

        Poll::Pending
    }
}

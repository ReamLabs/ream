use discv5::Executor;
use libp2p::{futures::StreamExt, identify, identity, noise, ping, swarm, PeerId, Swarm};
use libp2p::swarm::SwarmEvent;
use log::info;

use crate::config::Config;

pub struct NetworkBehaviour {
    pub identify: identify::Behaviour,

    pub ping: ping::Behaviour,
}

pub enum NetworkEvent {
    PeerConnectedIncoming(PeerId),
    PeerConnectedOutgoing(PeerId),
    PeerDisconnected(PeerId),
    Status(PeerId),
    Ping(PeerId),
    MetaData(PeerId),
    DisconnectPeer(PeerId),
    DiscoverPeers(usize),
}
pub struct Network {
    peer_id: PeerId,
    swarm: Swarm<NetworkBehaviour>,
}

impl Network {
    pub async fn init(config: &Config) -> Result<Network, String> {
        info!("Initializing network");
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                message = self.swarm.select_next_some() => {

                }
            }
        }
    }

    async fn start_network_worker(&mut self)-> Result<(), String> {

    }

    /// polling the libp2p swarm for network events.
    pub async fn polling_events(&mut self) -> NetworkEvent{
        loop{
            tokio::select! {
                Some(event) = self.swarm.select_next_some() => {
                    if let Some(event) = self.parse_swarm_event(event){
                        return event;
                    }
                }
            }
        }
    }

    fn parse_swarm_event(
        &mut self,
        event: SwarmEvent<NetworkEvent>,
    ) -> Option<NetworkEvent> {

        // currently no-op for any network events
        match event {
            SwarmEvent::Behaviour(behaviour_event) => match behaviour_event {
                NetworkEvent::PeerConnectedIncoming(_) => {
                    None
                }
                NetworkEvent::PeerConnectedOutgoing(_) => {
                    None
                }
                NetworkEvent::PeerDisconnected(_) => {
                    None
                }
                NetworkEvent::Status(_) => {
                    None
                }
                NetworkEvent::Ping(_) => {
                    None
                }
                NetworkEvent::MetaData(_) => {
                    None
                }
                NetworkEvent::DisconnectPeer(_) => {
                    None
                }
                NetworkEvent::DiscoverPeers(_) => {
                    None
                }
            }
            SwarmEvent::ConnectionEstablished { .. } => {
                None
            }
            SwarmEvent::ConnectionClosed { .. } => {
                None
            }
            SwarmEvent::IncomingConnection { .. } => {
                None
            }
            SwarmEvent::IncomingConnectionError { .. } => {
                None
            }
            SwarmEvent::OutgoingConnectionError { .. } => {
                None
            }
            SwarmEvent::NewListenAddr { .. } => {
                None
            }
            SwarmEvent::ExpiredListenAddr { .. } => {
                None
            }
            SwarmEvent::ListenerClosed { .. } => {
                None
            }
            SwarmEvent::ListenerError { .. } => {
                None
            }
            SwarmEvent::Dialing { .. } => {
                None
            }
            SwarmEvent::NewExternalAddrCandidate { .. } => {
                None
            }
            SwarmEvent::ExternalAddrConfirmed { .. } => {
                None
            }
            SwarmEvent::ExternalAddrExpired { .. } => {
                None
            }
            SwarmEvent::NewExternalAddrOfPeer { .. } => {
                None
            }
            _ => { None }
        }
    }
}

fn build_swarm(

) -> Swarm<NetworkBehaviour> {

}


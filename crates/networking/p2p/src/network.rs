use libp2p::{futures::StreamExt, identify, ping, PeerId, Swarm};
use log::info;

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
    behaviour: Swarm<NetworkBehaviour>,
}

impl Network {
    pub async fn init() -> Result<Network, String> {
        info!("Initializing network");
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                message = self.behaviour.select_next_some() => {

                }
            }
        }
    }
}

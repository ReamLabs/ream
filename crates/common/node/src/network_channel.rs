use tokio::sync::{mpsc, oneshot};

use crate::peer::PeerCountData;

#[derive(Debug)]
pub enum NetworkRequest {
    GetPeerCount(oneshot::Sender<PeerCountData>),
}

#[derive(Clone)]
pub struct NetworkChannel {
    sender: mpsc::Sender<NetworkRequest>,
}

impl NetworkChannel {
    pub fn new(sender: mpsc::Sender<NetworkRequest>) -> Self {
        Self { sender }
    }

    pub async fn get_peer_count(&self) -> Result<PeerCountData, anyhow::Error> {
        let (response_tx, response_rx) = oneshot::channel();
        self.sender
            .send(NetworkRequest::GetPeerCount(response_tx))
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send network request"))?;
        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("Failed to receive network response"))
    }
}

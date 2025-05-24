use libp2p::PeerId;
use tokio::sync::mpsc;

use crate::req_resp::messages::ResponseMessage;

pub enum P2PResponse {
    ResponseMessage(ResponseMessage),
    EndOfStream,
}

pub enum P2PMessages {
    RequestBlockRange {
        peer_id: PeerId,
        start: u64,
        count: u64,
        callback: mpsc::Sender<anyhow::Result<P2PResponse>>,
    },
}

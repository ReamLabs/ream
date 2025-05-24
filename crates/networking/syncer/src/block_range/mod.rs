use std::sync::Arc;

use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_p2p::channel::P2PMessage;
use tokio::sync::mpsc::UnboundedSender;

pub struct BlockRangeSyncer {
    pub beacon_chain: Arc<BeaconChain>,
    pub p2p_sender: UnboundedSender<P2PMessage>,
}

impl BlockRangeSyncer {
    pub fn new(beacon_chain: Arc<BeaconChain>, p2p_sender: UnboundedSender<P2PMessage>) -> Self {
        Self {
            beacon_chain,
            p2p_sender,
        }
    }
}

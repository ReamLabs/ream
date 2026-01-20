use alloy_primitives::B256;
use libp2p_identity::PeerId;

#[derive(Debug, Clone)]
pub enum PendingJobRequest {
    Reset {
        peer_id: PeerId,
    },
    Initial {
        root: B256,
        slot: u64,
        parent_root: B256,
    },
}

impl PendingJobRequest {
    pub fn new_reset(peer_id: PeerId) -> Self {
        PendingJobRequest::Reset { peer_id }
    }

    pub fn new_initial(root: B256, slot: u64, parent_root: B256) -> Self {
        PendingJobRequest::Initial {
            root,
            slot,
            parent_root,
        }
    }
}

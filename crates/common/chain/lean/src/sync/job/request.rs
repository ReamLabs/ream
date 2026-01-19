use std::time::Instant;

use alloy_primitives::B256;
use libp2p_identity::PeerId;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct JobRequest {
    pub peer_id: PeerId,
    pub root: B256,
    pub has_been_requested: bool,
    pub time_requested: Option<Instant>,
}

impl JobRequest {
    pub fn new(peer_id: PeerId, root: B256) -> Self {
        JobRequest {
            peer_id,
            root,
            has_been_requested: false,
            time_requested: None,
        }
    }

    pub fn mark_requested(&mut self) {
        self.has_been_requested = true;
        self.time_requested = Some(Instant::now());
    }
}

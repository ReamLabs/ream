pub mod cached_peer;

use std::{collections::HashMap, sync::Arc};

use libp2p::PeerId;
use parking_lot::{Mutex, RwLock};
use ream_consensus_lean::checkpoint::Checkpoint;
use ream_peer::ConnectionState;

use crate::cached_peer::CachedPeer;

#[derive(Debug)]
pub struct NetworkState {
    pub peer_table: Arc<Mutex<HashMap<PeerId, CachedPeer>>>,
    pub head_checkpoint: RwLock<Checkpoint>,
    pub finalized_checkpoint: RwLock<Checkpoint>,
}

impl NetworkState {
    pub fn new(head_checkpoint: Checkpoint, finalized_checkpoint: Checkpoint) -> Self {
        Self {
            peer_table: Arc::new(Mutex::new(HashMap::new())),
            head_checkpoint: RwLock::new(head_checkpoint),
            finalized_checkpoint: RwLock::new(finalized_checkpoint),
        }
    }

    pub fn update_peer_state(&self, peer_id: PeerId, state: ConnectionState) {
        self.peer_table
            .lock()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                cached_peer.state = state;
            });
    }
}

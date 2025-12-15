pub mod cached_peer;

use std::{collections::HashMap, sync::Arc};

use libp2p::{Multiaddr, PeerId};
use parking_lot::{Mutex, RwLock};
use ream_consensus_lean::checkpoint::Checkpoint;
use ream_peer::{ConnectionState, Direction};

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

    pub fn upsert_peer(
        &self,
        peer_id: PeerId,
        address: Option<Multiaddr>,
        state: ConnectionState,
        direction: Direction,
    ) {
        self.peer_table
            .lock()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                if let Some(address_ref) = &address {
                    cached_peer.last_seen_p2p_address = Some(address_ref.clone());
                }
                cached_peer.state = state;
                cached_peer.direction = direction;
            })
            .or_insert(CachedPeer::new(peer_id, address, state, direction));
    }

    pub fn connected_peers(&self) -> usize {
        self.peer_table
            .lock()
            .values()
            .filter(|peer| matches!(peer.state, ConnectionState::Connected))
            .count()
    }

    /// Returns the cached peer from the peer table.
    pub fn cached_peer(&self, id: &PeerId) -> Option<CachedPeer> {
        self.peer_table.lock().get(id).cloned()
    }
}

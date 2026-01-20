pub mod cached_peer;

use std::{collections::HashMap, sync::Arc, time::Instant};

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

    pub fn connected_peer_count(&self) -> usize {
        self.peer_table
            .lock()
            .values()
            .filter(|peer| matches!(peer.state, ConnectionState::Connected))
            .count()
    }

    pub fn connected_peer_ids_with_scores(&self) -> Vec<(PeerId, u8)> {
        self.peer_table
            .lock()
            .values()
            .filter(|peer| matches!(peer.state, ConnectionState::Connected))
            .map(|peer| (peer.peer_id, peer.peer_score))
            .collect()
    }

    /// Returns the cached peer from the peer table.
    pub fn cached_peer(&self, id: &PeerId) -> Option<CachedPeer> {
        self.peer_table.lock().get(id).cloned()
    }

    pub fn update_peer_checkpoints(
        &self,
        peer_id: PeerId,
        head_checkpoint: Checkpoint,
        finalized_checkpoint: Checkpoint,
    ) {
        if let Some(cached_peer) = self.peer_table.lock().get_mut(&peer_id) {
            cached_peer.head_checkpoint = Some(head_checkpoint);
            cached_peer.finalized_checkpoint = Some(finalized_checkpoint);
            cached_peer.last_status_update = Some(Instant::now());
        }
    }

    pub fn common_highest_checkpoint(&self) -> Option<Checkpoint> {
        let peer_table = self.peer_table.lock();
        let mut common_checkpoint: Option<Checkpoint> = None;

        let mut checkpoint_tally: HashMap<Checkpoint, usize> = HashMap::new();
        for peer in peer_table.values() {
            if let (ConnectionState::Connected, Some(head_checkpoint)) =
                (&peer.state, &peer.head_checkpoint)
            {
                *checkpoint_tally.entry(*head_checkpoint).or_insert(0) += 1;
            }
        }
        let mut highest_tally = 0;
        for (checkpoint, tally) in checkpoint_tally {
            if tally > highest_tally {
                highest_tally = tally;
                common_checkpoint = Some(checkpoint);
            }
        }

        common_checkpoint
    }

    pub fn successful_response_from_peer(&self, peer_id: PeerId) {
        if let Some(cached_peer) = self.peer_table.lock().get_mut(&peer_id) {
            cached_peer.peer_score = cached_peer.peer_score.saturating_add(10);
        }
    }

    pub fn failed_response_from_peer(&self, peer_id: PeerId) {
        if let Some(cached_peer) = self.peer_table.lock().get_mut(&peer_id) {
            cached_peer.peer_score = cached_peer.peer_score.saturating_sub(20);
        }
    }
}

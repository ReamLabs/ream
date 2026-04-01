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

    pub fn connected_peer_ids_with_scores_matching_head(
        &self,
        checkpoint: Checkpoint,
    ) -> Vec<(PeerId, u8)> {
        self.peer_table
            .lock()
            .values()
            .filter(|peer| {
                matches!(peer.state, ConnectionState::Connected)
                    && peer.head_checkpoint == Some(checkpoint)
            })
            .map(|peer| (peer.peer_id, peer.peer_score))
            .collect()
    }

    pub fn connected_peer_ids_with_scores_at_or_above_slot(
        &self,
        min_slot: u64,
    ) -> Vec<(PeerId, u8)> {
        self.peer_table
            .lock()
            .values()
            .filter(|peer| {
                matches!(peer.state, ConnectionState::Connected)
                    && peer
                        .head_checkpoint
                        .is_some_and(|checkpoint| checkpoint.slot >= min_slot)
            })
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
        self.common_checkpoint_by(|peer| peer.head_checkpoint)
    }

    pub fn preferred_highest_checkpoint(&self) -> Option<Checkpoint> {
        if let Some(checkpoint) = self.common_highest_checkpoint() {
            return Some(checkpoint);
        }

        let connected_peers: Vec<_> = self
            .peer_table
            .lock()
            .values()
            .filter(|peer| matches!(peer.state, ConnectionState::Connected))
            .cloned()
            .collect();

        if connected_peers.len() > 2 {
            return None;
        }

        let head_candidates: Vec<_> = connected_peers
            .iter()
            .filter_map(|peer| peer.head_checkpoint.map(|checkpoint| (peer, checkpoint)))
            .collect();
        if head_candidates.is_empty() {
            return None;
        }

        if Self::common_checkpoint_from_peers(connected_peers.iter(), |peer| {
            peer.finalized_checkpoint
        })
        .is_some()
        {
            return head_candidates
                .into_iter()
                .max_by_key(|(peer, checkpoint)| {
                    (
                        checkpoint.slot,
                        peer.peer_score,
                        peer.last_status_update,
                        peer.peer_id.to_bytes(),
                    )
                })
                .map(|(_, checkpoint)| checkpoint);
        }

        head_candidates
            .into_iter()
            .max_by_key(|(peer, checkpoint)| {
                (
                    peer.peer_score,
                    checkpoint.slot,
                    peer.last_status_update,
                    peer.peer_id.to_bytes(),
                )
            })
            .map(|(_, checkpoint)| checkpoint)
    }

    pub fn common_finalized_checkpoint(&self) -> Option<Checkpoint> {
        self.common_checkpoint_by(|peer| peer.finalized_checkpoint)
    }

    fn common_checkpoint_by(
        &self,
        checkpoint_selector: impl Fn(&CachedPeer) -> Option<Checkpoint>,
    ) -> Option<Checkpoint> {
        let peer_table = self.peer_table.lock();
        Self::common_checkpoint_from_peers(
            peer_table
                .values()
                .filter(|peer| matches!(peer.state, ConnectionState::Connected)),
            checkpoint_selector,
        )
    }

    fn common_checkpoint_from_peers<'a>(
        peers: impl IntoIterator<Item = &'a CachedPeer>,
        checkpoint_selector: impl Fn(&CachedPeer) -> Option<Checkpoint>,
    ) -> Option<Checkpoint> {
        let mut checkpoint_tally: HashMap<Checkpoint, usize> = HashMap::new();
        for peer in peers {
            if let Some(checkpoint) = checkpoint_selector(peer) {
                *checkpoint_tally.entry(checkpoint).or_insert(0) += 1;
            }
        }

        let max_tally = checkpoint_tally.values().copied().max()?;
        if max_tally == 1 && checkpoint_tally.len() > 1 {
            return None;
        }

        checkpoint_tally
            .into_iter()
            .max_by_key(|(checkpoint, tally)| (*tally, checkpoint.slot))
            .map(|(checkpoint, _)| checkpoint)
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

#[cfg(test)]
mod tests {
    use libp2p::PeerId;
    use ream_peer::{ConnectionState, Direction};

    use super::*;

    fn checkpoint(byte: u8, slot: u64) -> Checkpoint {
        let _ = byte;
        Checkpoint {
            root: Default::default(),
            slot,
        }
    }

    #[test]
    fn common_highest_checkpoint_returns_none_for_singleton_outlier_tie() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();

        network_state.upsert_peer(
            peer_a,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        network_state.upsert_peer(
            peer_b,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        network_state.update_peer_checkpoints(peer_a, checkpoint(0x10, 40), checkpoint(0x20, 10));
        network_state.update_peer_checkpoints(peer_b, checkpoint(0x11, 224), checkpoint(0x21, 211));

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(network_state.common_finalized_checkpoint(), None);
    }

    #[test]
    fn common_highest_checkpoint_prefers_agreed_checkpoint_over_outlier() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();

        for peer_id in [peer_a, peer_b, peer_c] {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        let agreed_head = checkpoint(0x30, 40);
        let agreed_finalized = checkpoint(0x31, 30);
        network_state.update_peer_checkpoints(peer_a, agreed_head, agreed_finalized);
        network_state.update_peer_checkpoints(peer_b, agreed_head, agreed_finalized);
        network_state.update_peer_checkpoints(peer_c, checkpoint(0x40, 224), checkpoint(0x41, 211));

        assert_eq!(network_state.common_highest_checkpoint(), Some(agreed_head));
        assert_eq!(
            network_state.common_finalized_checkpoint(),
            Some(agreed_finalized)
        );
    }

    #[test]
    fn connected_peer_queries_filter_by_head_checkpoint_and_slot() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_c = PeerId::random();
        let target_head = checkpoint(0x30, 40);

        for peer_id in [peer_a, peer_b, peer_c] {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        network_state.update_peer_checkpoints(peer_a, target_head, checkpoint(0x20, 10));
        network_state.update_peer_checkpoints(peer_b, checkpoint(0x31, 39), checkpoint(0x21, 10));
        network_state.update_peer_checkpoints(peer_c, target_head, checkpoint(0x22, 10));

        let matching = network_state.connected_peer_ids_with_scores_matching_head(target_head);
        assert_eq!(matching.len(), 2);
        assert!(matching.iter().any(|(peer_id, _)| *peer_id == peer_a));
        assert!(matching.iter().any(|(peer_id, _)| *peer_id == peer_c));

        let at_or_above = network_state.connected_peer_ids_with_scores_at_or_above_slot(40);
        assert_eq!(at_or_above.len(), 2);
        assert!(at_or_above.iter().any(|(peer_id, _)| *peer_id == peer_a));
        assert!(at_or_above.iter().any(|(peer_id, _)| *peer_id == peer_c));
        assert!(!at_or_above.iter().any(|(peer_id, _)| *peer_id == peer_b));
    }

    #[test]
    fn preferred_highest_checkpoint_uses_small_devnet_fallback_for_two_peer_split() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let shared_finalized = checkpoint(0x20, 10);
        let lower_head = checkpoint(0x30, 40);
        let higher_head = checkpoint(0x31, 224);

        for peer_id in [peer_a, peer_b] {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        network_state.update_peer_checkpoints(peer_a, lower_head, shared_finalized);
        network_state.update_peer_checkpoints(peer_b, higher_head, shared_finalized);

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(higher_head)
        );
    }

    #[test]
    fn preferred_highest_checkpoint_falls_back_to_highest_scored_peer_when_finalized_split() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let peer_a_head = checkpoint(0x30, 40);
        let peer_b_head = checkpoint(0x31, 224);

        for peer_id in [peer_a, peer_b] {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        network_state.update_peer_checkpoints(peer_a, peer_a_head, checkpoint(0x20, 10));
        network_state.update_peer_checkpoints(peer_b, peer_b_head, checkpoint(0x21, 211));

        if let Some(cached_peer) = network_state.peer_table.lock().get_mut(&peer_a) {
            cached_peer.peer_score = 240;
        }
        if let Some(cached_peer) = network_state.peer_table.lock().get_mut(&peer_b) {
            cached_peer.peer_score = 120;
        }

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(network_state.common_finalized_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(peer_a_head)
        );
    }
}

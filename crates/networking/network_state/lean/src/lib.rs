pub mod cached_peer;

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use libp2p::{Multiaddr, PeerId};
use parking_lot::{Mutex, RwLock};
use ream_consensus_lean::checkpoint::Checkpoint;
use ream_peer::{ConnectionState, Direction};

use crate::cached_peer::CachedPeer;

const MODIFIED_Z_SCORE_NUMERATOR: u128 = 6_745;
const MODIFIED_Z_SCORE_OUTLIER_THRESHOLD: u128 = 35_000;

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

        let head_candidates: Vec<_> = connected_peers
            .iter()
            .filter_map(|peer| peer.head_checkpoint.map(|checkpoint| (peer, checkpoint)))
            .collect();
        if head_candidates.is_empty() {
            return None;
        }

        let has_common_finalized =
            Self::common_checkpoint_from_peers(connected_peers.iter(), |peer| {
                peer.finalized_checkpoint
            })
            .is_some();

        if head_candidates.len() <= 2 {
            if has_common_finalized {
                return Self::highest_slot_checkpoint(head_candidates);
            }

            return Self::highest_scored_checkpoint(head_candidates);
        }

        Self::clustered_highest_checkpoint(&head_candidates)
    }

    fn highest_slot_checkpoint<'a>(
        head_candidates: impl IntoIterator<Item = (&'a CachedPeer, Checkpoint)>,
    ) -> Option<Checkpoint> {
        head_candidates
            .into_iter()
            .max_by_key(|(peer, checkpoint)| {
                (
                    checkpoint.slot,
                    peer.peer_score,
                    peer.last_status_update,
                    peer.peer_id.to_bytes(),
                )
            })
            .map(|(_, checkpoint)| checkpoint)
    }

    fn highest_scored_checkpoint<'a>(
        head_candidates: impl IntoIterator<Item = (&'a CachedPeer, Checkpoint)>,
    ) -> Option<Checkpoint> {
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

    fn clustered_highest_checkpoint(
        head_candidates: &[(&CachedPeer, Checkpoint)],
    ) -> Option<Checkpoint> {
        let mut slots: Vec<_> = head_candidates
            .iter()
            .map(|(_, checkpoint)| checkpoint.slot)
            .collect();
        slots.sort_unstable();

        let median_slot_x2 = Self::median_slot_x2(&slots);
        let mut deviations_x2: Vec<_> = slots
            .iter()
            .map(|slot| Self::slot_deviation_x2(*slot, median_slot_x2))
            .collect();
        deviations_x2.sort_unstable();
        let median_deviation_x2 = Self::median_u128(&deviations_x2);

        Self::highest_slot_checkpoint(head_candidates.iter().copied().filter(|(_, checkpoint)| {
            Self::slot_is_cluster_member(checkpoint.slot, median_slot_x2, median_deviation_x2)
        }))
    }

    fn slot_is_cluster_member(slot: u64, median_slot_x2: u128, median_deviation_x2: u128) -> bool {
        let deviation_x2 = Self::slot_deviation_x2(slot, median_slot_x2);
        if median_deviation_x2 == 0 {
            return deviation_x2 == 0;
        }

        deviation_x2 * MODIFIED_Z_SCORE_NUMERATOR
            <= median_deviation_x2 * MODIFIED_Z_SCORE_OUTLIER_THRESHOLD
    }

    fn median_slot_x2(sorted_slots: &[u64]) -> u128 {
        let mid = sorted_slots.len() / 2;
        if sorted_slots.len().is_multiple_of(2) {
            u128::from(sorted_slots[mid - 1]) + u128::from(sorted_slots[mid])
        } else {
            u128::from(sorted_slots[mid]) * 2
        }
    }

    fn median_u128(sorted_values: &[u128]) -> u128 {
        let mid = sorted_values.len() / 2;
        if sorted_values.len().is_multiple_of(2) {
            (sorted_values[mid - 1] + sorted_values[mid]) / 2
        } else {
            sorted_values[mid]
        }
    }

    fn slot_deviation_x2(slot: u64, median_slot_x2: u128) -> u128 {
        (u128::from(slot) * 2).abs_diff(median_slot_x2)
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

#[derive(Clone, Debug)]
pub struct AggregatorState {
    enabled: Arc<AtomicBool>,
}

impl AggregatorState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, enabled: bool) -> bool {
        self.enabled.swap(enabled, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use libp2p::PeerId;
    use ream_peer::{ConnectionState, Direction};

    use super::*;

    fn checkpoint(byte: u8, slot: u64) -> Checkpoint {
        Checkpoint {
            root: [byte; 32].into(),
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

    #[test]
    fn preferred_highest_checkpoint_uses_highest_checkpoint_in_slot_cluster() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peers = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ];
        let shared_finalized = checkpoint(0x20, 10);

        for peer_id in peers {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        let preferred_head = checkpoint(0x33, 103);
        network_state.update_peer_checkpoints(peers[0], checkpoint(0x30, 100), shared_finalized);
        network_state.update_peer_checkpoints(peers[1], checkpoint(0x31, 101), shared_finalized);
        network_state.update_peer_checkpoints(peers[2], checkpoint(0x32, 102), shared_finalized);
        network_state.update_peer_checkpoints(peers[3], preferred_head, shared_finalized);

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(preferred_head)
        );
    }

    #[test]
    fn preferred_highest_checkpoint_avoids_high_slot_outlier() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peers = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ];
        let shared_finalized = checkpoint(0x20, 10);

        for peer_id in peers {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        let preferred_head = checkpoint(0x33, 103);
        network_state.update_peer_checkpoints(peers[0], checkpoint(0x30, 100), shared_finalized);
        network_state.update_peer_checkpoints(peers[1], checkpoint(0x31, 101), shared_finalized);
        network_state.update_peer_checkpoints(peers[2], checkpoint(0x32, 102), shared_finalized);
        network_state.update_peer_checkpoints(peers[3], preferred_head, shared_finalized);
        network_state.update_peer_checkpoints(peers[4], checkpoint(0x40, 500), shared_finalized);

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(preferred_head)
        );
    }

    #[test]
    fn preferred_highest_checkpoint_uses_slot_cluster_when_finalized_checkpoints_split() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peers = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ];

        for peer_id in peers {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        let preferred_head = checkpoint(0x33, 103);
        network_state.update_peer_checkpoints(
            peers[0],
            checkpoint(0x30, 100),
            checkpoint(0x50, 10),
        );
        network_state.update_peer_checkpoints(
            peers[1],
            checkpoint(0x31, 101),
            checkpoint(0x51, 11),
        );
        network_state.update_peer_checkpoints(
            peers[2],
            checkpoint(0x32, 102),
            checkpoint(0x52, 12),
        );
        network_state.update_peer_checkpoints(peers[3], preferred_head, checkpoint(0x53, 13));
        network_state.update_peer_checkpoints(
            peers[4],
            checkpoint(0x40, 500),
            checkpoint(0x60, 400),
        );

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(network_state.common_finalized_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(preferred_head)
        );
    }

    #[test]
    fn preferred_highest_checkpoint_avoids_low_slot_outlier() {
        let network_state = NetworkState::new(checkpoint(0x01, 0), checkpoint(0x01, 0));
        let peers = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ];
        let shared_finalized = checkpoint(0x20, 10);

        for peer_id in peers {
            network_state.upsert_peer(
                peer_id,
                None,
                ConnectionState::Connected,
                Direction::Outbound,
            );
        }

        let preferred_head = checkpoint(0x34, 504);
        network_state.update_peer_checkpoints(peers[0], checkpoint(0x20, 100), shared_finalized);
        network_state.update_peer_checkpoints(peers[1], checkpoint(0x30, 500), shared_finalized);
        network_state.update_peer_checkpoints(peers[2], checkpoint(0x31, 501), shared_finalized);
        network_state.update_peer_checkpoints(peers[3], checkpoint(0x32, 502), shared_finalized);
        network_state.update_peer_checkpoints(peers[4], preferred_head, shared_finalized);

        assert_eq!(network_state.common_highest_checkpoint(), None);
        assert_eq!(
            network_state.preferred_highest_checkpoint(),
            Some(preferred_head)
        );
    }
}

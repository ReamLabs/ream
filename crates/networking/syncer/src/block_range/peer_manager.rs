use std::{
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
    time::{Duration, Instant},
};

use alloy_primitives::B256;
use libp2p::PeerId;
use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;
use ream_p2p::network::beacon::{network_state::NetworkState, peer::CachedPeer};
use tracing::warn;

pub const BAN_DURATION: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub enum PeerStatus {
    Idle,
    Downloading,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer: CachedPeer,
    pub peer_status: PeerStatus,
}

pub struct PeerManager {
    network_state: Arc<NetworkState>,
    peers: HashMap<PeerId, PeerInfo>,
    banned_peers: HashMap<PeerId, Instant>,
    ban_reasons: HashMap<PeerId, String>,
}

impl PeerManager {
    pub fn new(network_state: Arc<NetworkState>) -> Self {
        Self {
            network_state,
            peers: HashMap::new(),
            banned_peers: HashMap::new(),
            ban_reasons: HashMap::new(),
        }
    }

    pub fn update_peer_set(&mut self) {
        // Unban peers whose ban duration has elapsed
        self.banned_peers
            .retain(|_, banned_at| banned_at.elapsed() < BAN_DURATION);

        let connected_peers = self.network_state.connected_peers();
        for peer in &connected_peers {
            if self.banned_peers.contains_key(&peer.peer_id) {
                continue;
            }

            match self.peers.entry(peer.peer_id) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().peer = peer.clone();
                }
                Entry::Vacant(entry) => {
                    entry.insert(PeerInfo {
                        peer: peer.clone(),
                        peer_status: PeerStatus::Idle,
                    });
                }
            }
        }

        // Remove disconnected peers
        self.peers
            .retain(|peer_id, _| connected_peers.iter().any(|peer| peer.peer_id == *peer_id));
    }

    /// Bans a peer with a timestamp for expiration
    pub fn ban_peer(&mut self, peer_id: &PeerId, reason: String) {
        self.ban_reasons.insert(*peer_id, reason);
        if let Some(peer_info) = self.peers.remove(peer_id) {
            self.banned_peers
                .insert(peer_info.peer.peer_id, Instant::now());
        } else {
            warn!("Attempted to ban a peer that is not in the peer set: {peer_id}");
        }
    }

    /// Fetches an idle peer from the peer set.
    ///
    /// Will set the peer status to `Downloading` if an idle peer is found.
    pub fn fetch_idle_peer(&mut self) -> Option<CachedPeer> {
        for peer_info in self.peers.values_mut() {
            if let PeerStatus::Idle = peer_info.peer_status {
                peer_info.peer_status = PeerStatus::Downloading;
                return Some(peer_info.peer.clone());
            }
        }
        None
    }

    /// Fetches an idle peer whose advertised head slot is at least `min_slot`.
    /// Falls back to any idle peer if no peer specifically advertises `>= min_slot`.
    pub fn fetch_idle_peer_for_slot(&mut self, min_slot: u64) -> Option<CachedPeer> {
        for peer_info in self.peers.values_mut() {
            if let PeerStatus::Idle = peer_info.peer_status {
                if let Some(status) = &peer_info.peer.status
                    && status.head_slot < min_slot
                {
                    continue;
                }
                peer_info.peer_status = PeerStatus::Downloading;
                return Some(peer_info.peer.clone());
            }
        }

        self.fetch_idle_peer()
    }

    pub fn peer_counts(&self) -> String {
        let total_peers = self.peers.len();
        let idle_peers = self
            .peers
            .values()
            .filter(|peer_info| matches!(peer_info.peer_status, PeerStatus::Idle))
            .count();
        let downloading_peers = total_peers - idle_peers;

        format!(
            "Total Peers: {total_peers}, Idle: {idle_peers}, Downloading: {downloading_peers}, Banned: {}",
            self.banned_peers.len()
        )
    }

    /// Marks a peer as idle after a download is complete.
    pub fn mark_peer_as_idle(&mut self, peer_id: &PeerId) {
        if let Some(peer_info) = self.peers.get_mut(peer_id) {
            peer_info.peer_status = PeerStatus::Idle;
        }
    }

    /// Identifies the majority canonical finalized checkpoint `(finalized_root, finalized_epoch)`
    /// among connected peers.
    pub fn canonical_checkpoint(&self) -> Option<(B256, u64)> {
        let mut frequencies = HashMap::new();

        for peer in self.peers.values() {
            if let Some(status) = &peer.peer.status {
                *frequencies
                    .entry((status.finalized_root, status.finalized_epoch))
                    .or_insert(0) += 1;
            }
        }

        frequencies
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(checkpoint, _)| checkpoint)
    }

    /// Computes the finalized slot based on the majority canonical checkpoint.
    pub fn finalized_slot(&self) -> Option<u64> {
        self.canonical_checkpoint()
            .map(|(_, epoch)| epoch * SLOTS_PER_EPOCH)
    }

    /// Computes the canonical head slot reported by peers that agree on the canonical checkpoint.
    pub fn head_slot(&self) -> Option<u64> {
        let canonical_cp = self.canonical_checkpoint()?;
        self.peers
            .values()
            .filter_map(|peer| {
                let status = peer.peer.status.as_ref()?;
                if (status.finalized_root, status.finalized_epoch) == canonical_cp {
                    Some(status.head_slot)
                } else {
                    None
                }
            })
            .max()
    }

    /// Resolves the sync target slot based on current local progress:
    /// - If `local_slot < finalized_slot`: target `finalized_slot` (Finalized Range Sync phase)
    /// - If `local_slot >= finalized_slot`: target `head_slot` (Head Sync phase)
    pub fn sync_target_slot(&self, local_slot: u64) -> Option<u64> {
        let finalized = self.finalized_slot()?;
        let head = self.head_slot().unwrap_or(finalized);

        if local_slot < finalized {
            Some(finalized)
        } else {
            Some(head.max(finalized))
        }
    }
}

#[cfg(test)]
impl PeerManager {
    pub fn new_test() -> Self {
        use discv5::enr::{CombinedKey, Enr};
        use parking_lot::RwLock;
        use std::path::PathBuf;

        let key = CombinedKey::generate_secp256k1();
        let enr = Enr::builder().build(&key).expect("test ENR");
        let network_state = Arc::new(NetworkState {
            local_enr: RwLock::new(enr),
            peer_table: RwLock::new(HashMap::new()),
            meta_data: RwLock::new(
                ream_req_resp::beacon::messages::meta_data::GetMetaDataV3::default(),
            ),
            status: RwLock::new(ream_req_resp::beacon::messages::status::Status::default()),
            data_dir: PathBuf::from("test_data"),
        });

        Self::new(network_state)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::aliases::B32;
    use ream_p2p::network::beacon::peer::CachedPeer;
    use ream_peer::{ConnectionState, Direction};
    use ream_req_resp::beacon::messages::status::Status;

    use super::*;

    fn make_test_peer(
        peer_id: PeerId,
        finalized_root: B256,
        finalized_epoch: u64,
        head_slot: u64,
    ) -> CachedPeer {
        let mut peer = CachedPeer::new(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Inbound,
            None,
        );
        peer.status = Some(Status {
            fork_digest: B32::ZERO,
            finalized_root,
            finalized_epoch,
            head_root: B256::ZERO,
            head_slot,
            earliest_available_slot: 0,
        });
        peer
    }

    #[test]
    fn canonical_checkpoint_and_head_slot_resolution() {
        let mut peer_manager = PeerManager::new_test();

        let root_a = B256::repeat_byte(1);
        let root_b = B256::repeat_byte(2);

        let p1 = make_test_peer(PeerId::random(), root_a, 10, 350);
        let p2 = make_test_peer(PeerId::random(), root_a, 10, 360);
        let p3 = make_test_peer(PeerId::random(), root_b, 8, 300);

        peer_manager.peers.insert(
            p1.peer_id,
            PeerInfo {
                peer: p1,
                peer_status: PeerStatus::Idle,
            },
        );
        peer_manager.peers.insert(
            p2.peer_id,
            PeerInfo {
                peer: p2,
                peer_status: PeerStatus::Idle,
            },
        );
        peer_manager.peers.insert(
            p3.peer_id,
            PeerInfo {
                peer: p3,
                peer_status: PeerStatus::Idle,
            },
        );

        assert_eq!(peer_manager.canonical_checkpoint(), Some((root_a, 10)));
        assert_eq!(peer_manager.finalized_slot(), Some(10 * SLOTS_PER_EPOCH));
        assert_eq!(peer_manager.head_slot(), Some(360));

        // When local slot is before finalized slot (320), target finalized slot (320)
        assert_eq!(peer_manager.sync_target_slot(100), Some(320));
        // When local slot is at or past finalized slot, target head slot (360)
        assert_eq!(peer_manager.sync_target_slot(320), Some(360));
        assert_eq!(peer_manager.sync_target_slot(350), Some(360));
    }

    #[test]
    fn slot_aware_idle_peer_selection() {
        let mut peer_manager = PeerManager::new_test();
        let root = B256::repeat_byte(1);

        let p1 = make_test_peer(PeerId::random(), root, 10, 150);
        let p2 = make_test_peer(PeerId::random(), root, 10, 200);

        peer_manager.peers.insert(
            p1.peer_id,
            PeerInfo {
                peer: p1.clone(),
                peer_status: PeerStatus::Idle,
            },
        );
        peer_manager.peers.insert(
            p2.peer_id,
            PeerInfo {
                peer: p2.clone(),
                peer_status: PeerStatus::Idle,
            },
        );

        // Asking for slot 180 should select p2, not p1
        let selected = peer_manager.fetch_idle_peer_for_slot(180).unwrap();
        assert_eq!(selected.peer_id, p2.peer_id);
    }

    #[test]
    fn ban_cooldown_unbans_expired_peers() {
        let mut peer_manager = PeerManager::new_test();
        let peer_id = PeerId::random();

        // Directly insert an expired ban into banned_peers
        peer_manager.banned_peers.insert(
            peer_id,
            Instant::now() - (BAN_DURATION + Duration::from_secs(1)),
        );

        peer_manager.update_peer_set();
        assert!(!peer_manager.banned_peers.contains_key(&peer_id));
    }
}

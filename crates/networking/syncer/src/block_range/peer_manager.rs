use std::{collections::HashMap, sync::Arc, time::Instant};

use libp2p::PeerId;
use ream_consensus_misc::constants::beacon::SLOTS_PER_EPOCH;
use ream_p2p::network::beacon::{network_state::NetworkState, peer::CachedPeer};
use tracing::warn;

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
        let connected_peers = self.network_state.connected_peers();
        for peer in &connected_peers {
            if self.banned_peers.contains_key(&peer.peer_id) {
                continue;
            }

            self.peers.entry(peer.peer_id).or_insert_with(|| PeerInfo {
                peer: peer.clone(),
                peer_status: PeerStatus::Idle,
            });
        }

        // Remove disconnected peers
        self.peers
            .retain(|peer_id, _| connected_peers.iter().any(|peer| peer.peer_id == *peer_id));
    }

    /// Bans a peer
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

    pub fn finalized_slot(&self) -> Option<u64> {
        let mut frequencies = HashMap::new();

        for peer in self.peers.values() {
            if let Some(status) = &peer.peer.status {
                *frequencies
                    .entry(status.finalized_epoch * SLOTS_PER_EPOCH)
                    .or_insert(0) += 1;
            }
        }

        frequencies
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(slot, _)| slot)
    }
}

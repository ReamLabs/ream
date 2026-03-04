use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::anyhow;
use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use parking_lot::RwLock;
use ream_consensus_beacon::custody_group::{
    compute_columns_for_custody_group, get_custody_group_indices,
};
use ream_consensus_misc::constants::beacon::NUM_CUSTODY_GROUPS;
use ream_peer::{ConnectionState, Direction};
use ream_req_resp::beacon::messages::{meta_data::GetMetaDataV3, status::Status};
use ssz::Encode;
use tracing::trace;

use super::{peer::CachedPeer, utils::META_DATA_FILE_NAME};

pub struct NetworkState {
    pub local_enr: RwLock<Enr>,
    pub peer_table: RwLock<HashMap<PeerId, CachedPeer>>,
    pub meta_data: RwLock<GetMetaDataV3>,
    pub status: RwLock<Status>,
    pub data_dir: PathBuf,
}

impl NetworkState {
    pub fn upsert_peer(
        &self,
        peer_id: PeerId,
        address: Option<Multiaddr>,
        state: ConnectionState,
        direction: Direction,
        enr: Option<Enr>,
    ) {
        self.peer_table
            .write()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                if let Some(address_ref) = &address {
                    cached_peer.last_seen_p2p_address = Some(address_ref.clone());
                }
                cached_peer.state = state;
                cached_peer.direction = direction;
                if let Some(enr_ref) = &enr {
                    cached_peer.enr = Some(enr_ref.clone());
                }
            })
            .or_insert(CachedPeer::new(peer_id, address, state, direction, enr));
    }

    pub fn update_peer_state(&self, peer_id: PeerId, state: ConnectionState) {
        self.peer_table
            .write()
            .entry(peer_id)
            .and_modify(|cached_peer| {
                cached_peer.state = state;
            });
    }

    pub fn write_meta_data_to_disk(&self) -> anyhow::Result<()> {
        let meta_data_path = self.data_dir.join(META_DATA_FILE_NAME);
        fs::write(meta_data_path, self.meta_data.read().as_ssz_bytes())
            .map_err(|err| anyhow!("Failed to write meta data to disk: {err:?}"))?;
        Ok(())
    }

    /// Gets a vector of all connected peers.
    pub fn connected_peers(&self) -> Vec<CachedPeer> {
        self.peer_table
            .read()
            .values()
            .filter(|peer| peer.state == ConnectionState::Connected)
            .cloned()
            .collect()
    }
    /// Returns all connected peers that are expected to custody the given column.
    ///
    /// For each connected peer that has both an ENR and metadata (custody_group_count),
    /// computes their custody group indices and checks if the requested column falls within
    /// any of those groups.
    pub fn peers_for_column(&self, column_id: u64) -> Vec<PeerId> {
        let custody_group_for_column = column_id % NUM_CUSTODY_GROUPS;
        let peer_table = self.peer_table.read();
        let mut result = Vec::new();

        for peer in peer_table.values() {
            if peer.state != ConnectionState::Connected {
                continue;
            }

            let Some(enr) = &peer.enr else {
                continue;
            };

            let Some(meta_data) = &peer.meta_data else {
                continue;
            };

            let custody_group_count = meta_data.custody_group_count;
            if custody_group_count == 0 {
                continue;
            }

            let node_id = enr.node_id();
            let Ok(custody_groups) = get_custody_group_indices(node_id, custody_group_count) else {
                trace!(
                    "Failed to compute custody groups for peer {:?}",
                    peer.peer_id
                );
                continue;
            };

            if custody_groups.contains(&custody_group_for_column) {
                result.push(peer.peer_id);
            }
        }

        result
    }

    /// Returns all column indices that our local node custodies.
    pub fn local_custody_columns(&self) -> Vec<u64> {
        let local_enr = self.local_enr.read();
        let node_id = local_enr.node_id();
        let custody_group_count = self.meta_data.read().custody_group_count;

        if custody_group_count == 0 {
            return Vec::new();
        }

        let Ok(custody_groups) = get_custody_group_indices(node_id, custody_group_count) else {
            return Vec::new();
        };

        let mut columns = Vec::new();
        for group in custody_groups {
            if let Ok(group_columns) = compute_columns_for_custody_group(group) {
                columns.extend(group_columns);
            }
        }
        columns.sort();
        columns
    }

    /// Record a successful DAS sampling response from a peer.
    /// Increments the peer's success count and boosts their sampling score.
    pub fn record_sampling_success(&self, peer_id: PeerId) {
        if let Some(peer) = self.peer_table.write().get_mut(&peer_id) {
            peer.sampling_requests += 1;
            peer.sampling_successes += 1;
            peer.sampling_score = peer.sampling_score.saturating_add(10);
        }
    }

    /// Record a failed DAS sampling response from a peer.
    /// Increments the peer's failure count and penalizes their sampling score more
    /// aggressively than successes reward it.
    pub fn record_sampling_failure(&self, peer_id: PeerId) {
        if let Some(peer) = self.peer_table.write().get_mut(&peer_id) {
            peer.sampling_requests += 1;
            peer.sampling_failures += 1;
            peer.sampling_score = peer.sampling_score.saturating_sub(20);
        }
    }
}

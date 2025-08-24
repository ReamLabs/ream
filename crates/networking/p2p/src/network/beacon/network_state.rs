use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::anyhow;
use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use parking_lot::RwLock;
use ssz::Encode;

use super::{peer::CachedPeer, utils::META_DATA_FILE_NAME};
use crate::{
    network::peer::{ConnectionState, Direction},
    req_resp::beacon::messages::{meta_data::GetMetaDataV2, status::Status},
};

pub struct NetworkState {
    pub local_enr: RwLock<Enr>,
    pub peer_table: RwLock<HashMap<PeerId, CachedPeer>>,
    pub meta_data: RwLock<GetMetaDataV2>,
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
}

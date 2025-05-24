use std::{collections::HashMap, fs, path::PathBuf};

use anyhow::anyhow;
use discv5::Enr;
use libp2p::{Multiaddr, PeerId};
use parking_lot::RwLock;
use ssz::Encode;

use crate::{
    peer::{CachedPeer, ConnectionState, Direction},
    req_resp::messages::meta_data::GetMetaDataV2,
    utils::META_DATA_FILE_NAME,
};

pub struct NetworkState {
    pub peer_table: RwLock<HashMap<PeerId, CachedPeer>>,
    pub meta_data: RwLock<GetMetaDataV2>,
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
        let mut peer_table = self.peer_table.write();
        peer_table
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
            .or_insert(CachedPeer {
                peer_id,
                last_seen_p2p_address: address,
                state,
                direction,
                enr,
            });
    }

    pub fn write_meta_data_to_disk(&self) -> anyhow::Result<()> {
        let meta_data_path = self.data_dir.join(META_DATA_FILE_NAME);
        fs::write(meta_data_path, self.meta_data.read().as_ssz_bytes())
            .map_err(|err| anyhow!("Failed to write meta data to disk: {err:?}"))?;
        Ok(())
    }
}

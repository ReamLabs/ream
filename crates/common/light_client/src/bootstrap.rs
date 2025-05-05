use alloy_primitives::B256;
use anyhow::ensure;
use ream_consensus::{
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    sync_committee::SyncCommittee,
};
use serde::Serialize;
use ssz_types::{FixedVector, typenum::U5};
use tree_hash::TreeHash;

use crate::header::LightClientHeader;

#[derive(Serialize)]
pub struct LightClientBootstrap {
    pub header: LightClientHeader,
    pub current_sync_committee: SyncCommittee,
    pub current_sync_committee_branch: FixedVector<B256, U5>,
}

impl LightClientBootstrap {
    pub fn new(state: &BeaconState, signed_block: &SignedBeaconBlock) -> anyhow::Result<Self> {
        ensure!(
            state.slot == state.latest_block_header.slot,
            "State slot must be equal to block slot"
        );

        let mut header = state.latest_block_header.clone();
        header.state_root = state.tree_hash_root();

        ensure!(
            header.tree_hash_root() == signed_block.message.tree_hash_root(),
            "Header root must be equal to block root"
        );

        Ok(LightClientBootstrap {
            header: LightClientHeader::new(signed_block)?,
            current_sync_committee: (*state.current_sync_committee).clone(),
            current_sync_committee_branch: state.current_sync_committee_inclusion_proof()?.into(),
        })
    }
}

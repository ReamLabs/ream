use alloy_primitives::B256;
use anyhow::ensure;
use ream_consensus::{
    constants::GENESIS_SLOT,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    misc::compute_sync_committee_period_at_slot,
    sync_aggregate::SyncAggregate,
    sync_committee::SyncCommittee,
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U6};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::header::LightClientHeader;

pub const MIN_SYNC_COMMITTEE_PARTICIPANTS: u64 = 1;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LightClientUpdate {
    /// Header attested to by the sync committee
    pub attested_header: LightClientHeader,
    /// Next sync committee corresponding to `attested_header.beacon.state_root`
    pub next_sync_committee: SyncCommittee,
    pub next_sync_committee_branch: FixedVector<B256, U6>,
    /// Finalized header corresponding to `attested_header.beacon.state_root`
    pub finalized_header: LightClientHeader,
    pub finality_branch: FixedVector<B256, U6>,
    /// Sync committee aggregate signature
    pub sync_aggregate: SyncAggregate,
    /// Slot at which the aggregate signature was created (untrusted)
    #[serde(with = "serde_utils::quoted_u64")]
    pub signature_slot: u64,
}

impl LightClientUpdate {
    pub fn new(
        state: BeaconState,
        block: SignedBeaconBlock,
        attested_state: BeaconState,
        attested_block: SignedBeaconBlock,
        finalized_block: Option<SignedBeaconBlock>,
    ) -> anyhow::Result<Self> {
        ensure!(
            block
                .message
                .body
                .sync_aggregate
                .sync_committee_bits
                .iter()
                .filter(|sync_committee_bit| *sync_committee_bit)
                .count() as u64
                >= MIN_SYNC_COMMITTEE_PARTICIPANTS,
            "Not enough sync committee participants"
        );
        ensure!(
            state.slot == state.latest_block_header.slot,
            "State slot must be equal to block slot"
        );

        let mut header = state.latest_block_header.clone();
        header.state_root = state.tree_hash_root();

        ensure!(
            header.tree_hash_root() == block.message.tree_hash_root(),
            "Header root must be equal to block root"
        );
        let update_signature_period = compute_sync_committee_period_at_slot(block.message.slot);
        ensure!(attested_state.slot == attested_state.latest_block_header.slot);
        let mut attested_header = attested_state.latest_block_header.clone();
        attested_header.state_root = attested_state.tree_hash_root();
        ensure!(
            attested_header.tree_hash_root() == attested_block.message.tree_hash_root()
                && attested_block.message.tree_hash_root() == block.message.parent_root,
            "Mismatch: attested_header, attested_block.message, or block.message.parent_root"
        );
        let update_attested_period =
            compute_sync_committee_period_at_slot(attested_block.message.slot);

        let attested_header = LightClientHeader::new(&attested_block)?;

        // `next_sync_committee` is only useful if the message is signed by the current sync
        // committee
        ensure!(
            update_signature_period == update_attested_period,
            "Signature period must match attested period"
        );
        let next_sync_committee = attested_state.next_sync_committee.as_ref().clone();
        let next_sync_committee_branch =
            attested_state.next_sync_committee_inclusion_proof()?.into();

        // Indicate finality whenever possible
        let (finalized_header, finality_branch) = match finalized_block {
            Some(finalized_block) => {
                let proof = attested_state.finalized_root_inclusion_proof()?.into();
                if finalized_block.message.slot != GENESIS_SLOT {
                    let header = LightClientHeader::new(&finalized_block)?;
                    ensure!(
                        header.beacon.tree_hash_root() == attested_state.finalized_checkpoint.root,
                        "Finalized header root does not match attested finalized checkpoint"
                    );
                    (header, proof)
                } else {
                    ensure!(
                        attested_state.finalized_checkpoint.root == B256::default(),
                        "Expected empty finalized checkpoint root at genesis"
                    );
                    (Default::default(), proof)
                }
            }
            None => (Default::default(), Default::default()),
        };
        Ok(LightClientUpdate {
            attested_header,
            next_sync_committee,
            next_sync_committee_branch,
            finalized_header,
            finality_branch,
            sync_aggregate: block.message.body.sync_aggregate,
            signature_slot: block.message.slot,
        })
    }
}

use alloy_primitives::B256;
use ream_consensus_beacon::sync_aggregate::SyncAggregate;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U6};
use tree_hash_derive::TreeHash;

use crate::header::LightClientHeader;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LightClientFinalityUpdate {
    pub attested_header: LightClientHeader,
    /// Finalized header corresponding to `attested_header.beacon.state_root`
    pub finalized_header: LightClientHeader,
    pub finality_branch: FixedVector<B256, U6>,
    /// Sync committee aggregate signature
    pub sync_aggregate: SyncAggregate,
    /// Slot at which the aggregate signature was created (untrusted)
    #[serde(with = "serde_utils::quoted_u64")]
    pub signature_slot: u64,
}

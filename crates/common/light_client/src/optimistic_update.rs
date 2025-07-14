use ream_consensus_beacon::sync_aggregate::SyncAggregate;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::header::LightClientHeader;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct LightClientOptimisticUpdate {
    /// Header attested to by the sync committee
    pub attested_header: LightClientHeader,
    /// Sync committee aggregate signature
    pub sync_aggregate: SyncAggregate,
    /// Slot at which the aggregate signature was created (untrusted)
    #[serde(with = "serde_utils::quoted_u64")]
    pub signature_slot: u64,
}

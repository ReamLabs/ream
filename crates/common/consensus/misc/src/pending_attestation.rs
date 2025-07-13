use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitList, typenum::U2048};
use tree_hash_derive::TreeHash;

use crate::attestation_data::AttestationData;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingAttestation {
    pub aggregation_bits: BitList<U2048>,
    pub data: AttestationData,
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_index: u64,
}

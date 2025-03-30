use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitList, typenum};
use tree_hash_derive::TreeHash;

use crate::attestation_data::AttestationData;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct PendingAttestation {
    pub aggregation_bits: BitList<typenum::U2048>,
    pub data: AttestationData,
    pub proposer_index: u64,
}

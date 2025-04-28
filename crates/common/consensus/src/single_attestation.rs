use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::attestation_data::AttestationData;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SingleAttestation {
    pub committee_index: u64,
    pub attester_index: u64,
    pub data: AttestationData,
    pub signature: BLSSignature,
}

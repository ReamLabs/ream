use ream_bls::BLSSignature;
use ream_consensus_misc::attestation_data::AttestationData;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SingleAttestation {
    pub committee_index: u64,
    pub attester_index: u64,
    pub data: AttestationData,
    pub signature: BLSSignature,
}

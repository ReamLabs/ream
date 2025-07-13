use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::indexed_attestation::IndexedAttestation;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttesterSlashing {
    pub attestation_1: IndexedAttestation,
    pub attestation_2: IndexedAttestation,
}

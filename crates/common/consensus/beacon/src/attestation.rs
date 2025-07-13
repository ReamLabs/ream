use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    BitList, BitVector,
    typenum::{U64, U131072},
};
use tree_hash_derive::TreeHash;

use crate::attestation_data::AttestationData;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Attestation {
    pub aggregation_bits: BitList<U131072>,
    pub data: AttestationData,
    pub signature: BLSSignature,
    pub committee_bits: BitVector<U64>,
}

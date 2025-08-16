use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, serde_utils::quoted_u64_var_list, typenum::U131072};
use tree_hash_derive::TreeHash;

use crate::attestation_data::AttestationData;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, Hash, TreeHash)]
pub struct IndexedAttestation {
    #[serde(with = "quoted_u64_var_list")]
    pub attesting_indices: VariableList<u64, U131072>,
    pub data: AttestationData,
    pub signature: BLSSignature,
}

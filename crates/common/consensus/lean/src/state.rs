use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::VariableList;
use tree_hash_derive::TreeHash;

use crate::validator::Validator;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeamState {
    pub genesis_time: u64,

    /// Up to 1 million validators
    pub validators: VariableList<Validator, ssz_types::typenum::U1000000>,
}

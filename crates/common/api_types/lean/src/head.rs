use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(Debug, Deserialize, Serialize, Encode, Decode)]
pub struct Head {
    pub head: B256,
}

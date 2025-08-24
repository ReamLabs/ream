use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Status {
    pub finalized_root: B256,
    pub finalized_slot: u64,
    pub head_root: B256,
    pub head_slot: u64,
}

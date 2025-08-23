use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};

/// Lean consensus status message according to the architecture specification
#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LeanStatus {
    pub finalized_root: B256,
    pub finalized_slot: u64,
    pub head_root: B256,
    pub head_slot: u64,
}

impl LeanStatus {
    pub fn new(finalized_root: B256, finalized_slot: u64, head_root: B256, head_slot: u64) -> Self {
        Self {
            finalized_root,
            finalized_slot,
            head_root,
            head_slot,
        }
    }
}

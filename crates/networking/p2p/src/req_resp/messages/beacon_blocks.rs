use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U1024};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BeaconBlocksByRangeV2Request {
    pub start_slot: u64,
    pub count: u64,
    /// Deprecated, must be set to 1
    step: u64,
}

impl BeaconBlocksByRangeV2Request {
    pub fn new(start_slot: u64, count: u64) -> Self {
        Self {
            start_slot,
            count,
            step: 1,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct BeaconBlocksByRootV2Request {
    pub inner: VariableList<B256, U1024>,
}

/// Will panic if over 1024 roots are requested
impl BeaconBlocksByRootV2Request {
    pub fn new(roots: Vec<B256>) -> Self {
        Self {
            inner: VariableList::new(roots).expect("Too many roots were requested"),
        }
    }
}

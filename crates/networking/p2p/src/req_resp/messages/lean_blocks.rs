use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U1024};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct LeanBlocksByRangeV1Request {
    pub start_slot: u64,
    pub count: u64,
}

impl LeanBlocksByRangeV1Request {
    pub fn new(start_slot: u64, count: u64) -> Self {
        Self { start_slot, count }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct LeanBlocksByRootV1Request {
    pub inner: VariableList<B256, U1024>,
}

impl LeanBlocksByRootV1Request {
    pub fn new(roots: Vec<B256>) -> Self {
        Self {
            inner: VariableList::new(roots).expect("Too many roots were requested"),
        }
    }
}

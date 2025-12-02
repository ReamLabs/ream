use alloy_primitives::B256;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U1024};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct AttestationSubnetRequest {
    pub subnet_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub needs_aggregator: bool,
}

impl AttestationSubnetRequest {
    pub fn new(subnet_id: u64, start_slot: u64, end_slot: u64, needs_aggregator: bool) -> Self {
        Self {
            subnet_id,
            start_slot,
            end_slot,
            needs_aggregator,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct AttestationSubnetResponse {
    pub inner: VariableList<B256, U1024>,
}

impl AttestationSubnetResponse {
    pub fn new(roots: Vec<B256>) -> Self {
        Self {
            inner: VariableList::new(roots).expect("Too many roots were requested"),
        }
    }
}

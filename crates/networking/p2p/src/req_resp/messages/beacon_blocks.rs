use alloy_primitives::B256;
use ream_consensus::electra::beacon_block::SignedBeaconBlock;
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U1024};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BeaconBlocksByRangeV2Request {
    pub start_slot: u64,
    pub count: u64,
    /// Deprecated, must be set to 1
    pub step: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct BeaconBlocksByRootV2Request {
    pub inner: VariableList<B256, U1024>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(struct_behaviour = "transparent")]
pub struct BeaconBlocksResponse {
    pub inner: VariableList<SignedBeaconBlock, U1024>,
}

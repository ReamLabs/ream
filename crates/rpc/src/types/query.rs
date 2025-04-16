use alloy_primitives::B256;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RandaoQuery {
    pub epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SlotQuery {
    pub slot: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RootQuery {
    pub root: Option<B256>,
}

#[derive(Debug, Deserialize)]
pub struct ParentRootQuery {
    pub parent_root: Option<B256>,
}

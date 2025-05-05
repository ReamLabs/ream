use alloy_primitives::{B256, aliases::B32};
use ssz_derive::{Decode, Encode};

#[derive(Debug, Default, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Status {
    pub fork_digest: B32,
    pub finalized_root: B256,
    pub finalized_epoch: u64,
    pub head_root: B256,
    pub head_epoch: u64,
}

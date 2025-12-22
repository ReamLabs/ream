use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

/// Represents a checkpoint in the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#checkpoint)
/// for detailed protocol information.
#[derive(
    Debug,
    Default,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Encode,
    Decode,
    TreeHash,
    Hash,
)]
pub struct Checkpoint {
    pub root: B256,
    pub slot: u64,
}

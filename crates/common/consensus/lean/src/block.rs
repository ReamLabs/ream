use alloy_primitives::{B256, FixedBytes};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::vote::SignedVote;

/// Represents a signed block in the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#signedblock)
/// for detailed protocol information.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBlock {
    pub message: Block,
    pub signature: FixedBytes<4000>,
}

/// Represents a block in the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#block)
/// for detailed protocol information.
#[derive(
    Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash,
)]
pub struct Block {
    pub slot: u64,
    pub proposer_index: u64,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub parent_root: B256,
    // Diverged from Python implementation: Disallow `None` (uses `B256::ZERO` instead)
    pub state_root: B256,
    pub body: BlockBody,
}

/// Represents a block header in the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#blockheader)
/// for detailed protocol information.
#[derive(
    Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash,
)]
pub struct BlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

impl From<Block> for BlockHeader {
    fn from(block: Block) -> Self {
        BlockHeader {
            slot: block.slot,
            proposer_index: block.proposer_index,
            parent_root: block.parent_root,
            state_root: block.state_root,
            body_root: block.body.tree_hash_root(),
        }
    }
}

/// Represents the body of a block in the Lean chain.
///
/// See the [Lean specification](https://github.com/leanEthereum/leanSpec/blob/main/docs/client/containers.md#blockbody)
/// for detailed protocol information.
#[derive(
    Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash,
)]
pub struct BlockBody {
    pub attestations: VariableList<SignedVote, U4096>,
}

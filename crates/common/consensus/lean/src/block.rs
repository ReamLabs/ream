use alloy_primitives::{B256, FixedBytes};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::attestation::Attestation;

/// Envelope carrying a block, an attestation from proposer, and aggregated signatures.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SignedBlockWithAttestation {
    pub message: BlockWithAttestation,
    pub signature: VariableList<FixedBytes<4000>, U4096>,
}

/// Bundle containing a block and the proposer's attestation.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct BlockWithAttestation {
    pub block: Block,
    pub proposer_attestation: Attestation,
}

/// Represents a block in the Lean chain.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
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
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
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
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlockBody {
    /// TODO: Diverged from current ongoing spec change. This should be
    /// `VariableList<Attestation, U4096>`.
    /// Tracking issue: https://github.com/ReamLabs/ream/issues/856
    pub attestations: VariableList<Attestation, U4096>,
}

use alloy_primitives::{B256, aliases::B32};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ForkData {
    pub current_version: B32,
    pub genesis_validators_root: B256,
}

impl ForkData {
    /// Return the 32-byte fork data root for the ``current_version`` and
    /// ``genesis_validators_root``. This is used primarily in signature domains to avoid
    /// collisions across forks/chains.
    pub fn compute_fork_data_root(&self) -> B256 {
        self.tree_hash_root()
    }

    /// Return the 4-byte fork digest for the ``current_version`` and ``genesis_validators_root``.
    /// This is a digest primarily used for domain separation on the p2p layer.
    /// 4-bytes suffices for practical separation of forks/chains.
    pub fn compute_fork_digest(&self) -> B32 {
        B32::from_slice(&self.compute_fork_data_root()[..4])
    }
}

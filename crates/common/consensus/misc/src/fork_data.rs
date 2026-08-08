use alloy_primitives::{B256, aliases::B32};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::blob_parameters::{BlobParameters, get_blob_parameters};

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
}

/// Return the 4-byte fork digest for the ``current_version`` and ``genesis_validators_root``.
/// This is a digest primarily used for domain separation on the p2p layer.
/// 4-bytes suffices for practical separation of forks/chains.
pub fn compute_fork_digest(
    fork_data: ForkData,
    blob_schedule: &[BlobParameters],
    fulu_fork_epoch: u64,
    epoch: u64,
) -> B32 {
    let base_digest = fork_data.compute_fork_data_root();

    if epoch < fulu_fork_epoch {
        return B32::from_slice(&base_digest[..4]);
    }

    let blob_parameters = get_blob_parameters(blob_schedule, epoch);

    // EIP-7892: SHA256 over the raw little-endian uint64 bytes, not the SSZ hash_tree_root.
    let mut hasher = Sha256::new();
    hasher.update(blob_parameters.epoch.to_le_bytes());
    hasher.update(blob_parameters.max_blobs_per_block.to_le_bytes());
    let blob_hash = hasher.finalize();

    let mut result = [0u8; 4];
    for index in 0..4 {
        result[index] = base_digest[index] ^ blob_hash[index];
    }

    B32::from_slice(&result)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::fixed_bytes;

    use super::*;

    fn schedule() -> Vec<BlobParameters> {
        vec![
            BlobParameters {
                epoch: 9,
                max_blobs_per_block: 9,
            },
            BlobParameters {
                epoch: 100,
                max_blobs_per_block: 100,
            },
            BlobParameters {
                epoch: 150,
                max_blobs_per_block: 175,
            },
        ]
    }

    fn fork_data() -> ForkData {
        ForkData {
            current_version: fixed_bytes!("0x06000000"),
            genesis_validators_root: B256::ZERO,
        }
    }

    /// Cross-check against the vector in Lighthouse's `blob_schedule_fork_digest`
    /// (`consensus/types/src/core/chain_spec.rs`). A mismatch means peers reject our
    /// `Status` and we subscribe to the wrong gossip topics, so this must stay
    /// byte-identical to other clients.
    #[test]
    fn test_compute_fork_digest_matches_other_clients() {
        assert_eq!(
            compute_fork_digest(fork_data(), &schedule(), 100, 100),
            fixed_bytes!("0xdf67557b"),
        );
    }

    /// The digest only changes on a BPO boundary, so every epoch between two entries
    /// resolves to the earlier one.
    #[test]
    fn test_compute_fork_digest_is_stable_between_bpo_entries() {
        assert_eq!(
            compute_fork_digest(fork_data(), &schedule(), 100, 149),
            compute_fork_digest(fork_data(), &schedule(), 100, 100),
        );
        assert_ne!(
            compute_fork_digest(fork_data(), &schedule(), 100, 150),
            compute_fork_digest(fork_data(), &schedule(), 100, 149),
        );
    }

    /// Before Fulu there is no BPO masking, so the digest is the first four bytes of the
    /// fork data root.
    #[test]
    fn test_compute_fork_digest_is_unmasked_before_fulu() {
        assert_eq!(
            compute_fork_digest(fork_data(), &schedule(), 100, 99),
            B32::from_slice(&fork_data().compute_fork_data_root()[..4]),
        );
    }
}

use alloy_primitives::{B256, aliases::B32, fixed_bytes};
use ethereum_ssz::Encode;
use ethereum_ssz_derive::{Decode, Encode};
use ream_consensus::fork_data::ForkData;

// Constants
pub const FAR_FUTURE_EPOCH: u64 = 18446744073709551615;
pub const GENESIS_FORK_VERSION: B32 = fixed_bytes!("0x00000000");
pub const GENESIS_VALIDATORS_ROOT: B256 =
    fixed_bytes!("0x0000000000000000000000000000000000000000000000000000000000000000");
pub const ENR_ETH2_KEY: &str = "eth2";
// Types
#[derive(Default, Debug, Encode, Decode)]
pub struct ENRForkID {
    pub fork_digest: B32,
    pub next_fork_version: B32,
    pub next_fork_epoch: u64,
}

impl ENRForkID {
    pub fn new(current_fork_version: B32, next_fork_version: B32, next_fork_epoch: u64) -> Self {
        let fork_digest = ForkData {
            current_version: current_fork_version,
            genesis_validators_root: GENESIS_VALIDATORS_ROOT,
        }
        .compute_fork_digest();

        Self {
            fork_digest,
            next_fork_version,
            next_fork_epoch,
        }
    }

    pub fn new_pectra() -> Self {
        // Pectra fork version and epoch values
        let current_fork_version = B32::from_slice(&[0x03, 0x00, 0x00, 0x00]); // This should be replaced with actual Pectra fork version
        let next_fork_version = B32::from_slice(&[0x00, 0x00, 0x00, 0x00]); // FAR_FUTURE_EPOCH version
        let next_fork_epoch = FAR_FUTURE_EPOCH;

        Self::new(current_fork_version, next_fork_version, next_fork_epoch)
    }

    pub fn as_ssz_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        Encode::encode(self, &mut bytes);
        bytes
    }
    pub fn compute_fork_digest(current_fork_version: B32) -> B32 {
        let fork_data = ForkData {
            current_version: current_fork_version,
            genesis_validators_root: GENESIS_VALIDATORS_ROOT,
        };
        fork_data.compute_fork_digest()
    }
}

impl Decode for ENRForkID {
    fn decode(bytes: &[u8]) -> Result<Self, ethereum_ssz::DecodeError> {
        ethereum_ssz::Decode::decode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper function
    fn compute_test_fork_digest(current_fork_version: B32) -> B32 {
        ENRForkID::compute_fork_digest(current_fork_version)
    }

    #[test]
    fn test_fork_digest_computation() {
        let current_fork_version = B32::from_slice(&[1, 2, 3, 4]);

        let fork_digest = ENRForkID::compute_fork_digest(current_fork_version);
        assert_eq!(fork_digest.len(), 32);

        let fork_digest2 = compute_test_fork_digest(current_fork_version);
        assert_eq!(fork_digest, fork_digest2);

        let different_version = B32::from_slice(&[2, 3, 4, 5]);
        let different_digest = compute_test_fork_digest(different_version);
        assert_ne!(fork_digest, different_digest);
    }

    #[test]
    fn test_serialization() {
        let fork_id = ENRForkID {
            fork_digest: B32::from_slice(&[1, 2, 3, 4]),
            next_fork_version: B32::from_slice(&[5, 6, 7, 8]),
            next_fork_epoch: 100,
        };

        let bytes = fork_id.as_ssz_bytes();
        let deserialized = ENRForkID::decode(&bytes).unwrap();

        assert_eq!(fork_id.fork_digest, deserialized.fork_digest);
        assert_eq!(fork_id.next_fork_version, deserialized.next_fork_version);
        assert_eq!(fork_id.next_fork_epoch, deserialized.next_fork_epoch);
    }
}

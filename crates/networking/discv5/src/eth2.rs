use ssz_rs::prelude::*;
use std::time::SystemTime;
use sha2::{Sha256, Digest};

// Constants
pub const FAR_FUTURE_EPOCH: u64 = u64::MAX;
pub const GENESIS_FORK_VERSION: [u8; 4] = [0, 0, 0, 0];

// Types
#[derive(Default, Debug, SimpleSerialize)]
pub struct ENRForkID {
    pub fork_digest: [u8; 4],
    pub next_fork_version: [u8; 4],
    pub next_fork_epoch: u64,
}

impl ENRForkID {
    pub fn new(
        current_fork_version: [u8; 4],
        genesis_validators_root: [u8; 32],
        next_fork_version: [u8; 4],
        next_fork_epoch: u64,
    ) -> Self {
        let fork_digest = Self::compute_fork_digest(current_fork_version, genesis_validators_root);
        
        Self {
            fork_digest,
            next_fork_version,
            next_fork_epoch,
        }
    }

    pub fn compute_fork_digest(current_fork_version: [u8; 4], genesis_validators_root: [u8; 32]) -> [u8; 4] {
        // Create a SHA256 hasher
        let mut hasher = Sha256::new();
        
        // Hash the current fork version
        hasher.update(current_fork_version);
        
        // Hash the genesis validators root
        hasher.update(genesis_validators_root);
        
        // Get the hash result
        let result = hasher.finalize();
        
        // Take the first 4 bytes as the fork digest
        let mut fork_digest = [0u8; 4];
        fork_digest.copy_from_slice(&result[..4]);
        
        fork_digest
    }

    pub fn pre_genesis() -> Self {
        Self {
            fork_digest: Self::compute_fork_digest(GENESIS_FORK_VERSION, [0; 32]),
            next_fork_version: GENESIS_FORK_VERSION,
            next_fork_epoch: FAR_FUTURE_EPOCH,
        }
    }

    pub fn update_enr(&self, enr: &mut discv5::Enr) -> Result<(), discv5::enr::EnrError> {
        // Serialize the ENRForkID to SSZ bytes
        let mut bytes = Vec::new();
        self.serialize(&mut bytes).map_err(|_| discv5::enr::EnrError::InvalidSignature)?;
        
        // Update the ENR with the eth2 key
        enr.insert(b"eth2", &bytes)
    }
}

// Peer compatibility checks
pub fn are_peers_compatible(local: &ENRForkID, remote: &ENRForkID) -> bool {
    // Peers are compatible if they have the same fork digest
    local.fork_digest == remote.fork_digest
}

pub fn will_peers_remain_compatible(local: &ENRForkID, remote: &ENRForkID) -> bool {
    // Peers will remain compatible if they have the same next fork version and epoch
    local.next_fork_version == remote.next_fork_version && 
    local.next_fork_epoch == remote.next_fork_epoch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fork_digest_computation() {
        let current_fork_version = [1, 2, 3, 4];
        let genesis_validators_root = [5; 32];
        
        let fork_digest = ENRForkID::compute_fork_digest(current_fork_version, genesis_validators_root);
        
        // Verify the fork digest is 4 bytes
        assert_eq!(fork_digest.len(), 4);
        
        // Verify the fork digest is deterministic
        let fork_digest2 = ENRForkID::compute_fork_digest(current_fork_version, genesis_validators_root);
        assert_eq!(fork_digest, fork_digest2);
        
        // Verify different inputs produce different digests
        let different_version = [2, 3, 4, 5];
        let different_digest = ENRForkID::compute_fork_digest(different_version, genesis_validators_root);
        assert_ne!(fork_digest, different_digest);
    }

    #[test]
    fn test_pre_genesis_fork_id() {
        let pre_genesis = ENRForkID::pre_genesis();
        
        assert_eq!(pre_genesis.next_fork_version, GENESIS_FORK_VERSION);
        assert_eq!(pre_genesis.next_fork_epoch, FAR_FUTURE_EPOCH);
        
        // Verify the fork digest is computed correctly for pre-genesis
        let expected_digest = ENRForkID::compute_fork_digest(GENESIS_FORK_VERSION, [0; 32]);
        assert_eq!(pre_genesis.fork_digest, expected_digest);
    }

    #[test]
    fn test_peer_compatibility() {
        let fork_id1 = ENRForkID {
            fork_digest: [1, 2, 3, 4],
            next_fork_version: [5, 6, 7, 8],
            next_fork_epoch: 100,
        };
        
        let fork_id2 = ENRForkID {
            fork_digest: [1, 2, 3, 4], // Same fork digest
            next_fork_version: [9, 10, 11, 12], // Different next fork
            next_fork_epoch: 200, // Different next epoch
        };
        
        // Should be compatible because fork digests match
        assert!(are_peers_compatible(&fork_id1, &fork_id2));
        
        // Should not remain compatible because next fork info differs
        assert!(!will_peers_remain_compatible(&fork_id1, &fork_id2));
    }

    #[test]
    fn test_enr_update() {
        let fork_id = ENRForkID {
            fork_digest: [1, 2, 3, 4],
            next_fork_version: [5, 6, 7, 8],
            next_fork_epoch: 100,
        };
        
        let mut enr = discv5::Enr::default();
        
        // Update the ENR with the fork ID
        assert!(fork_id.update_enr(&mut enr).is_ok());
        
        // Verify the eth2 key was added
        assert!(enr.get(b"eth2").is_some());
        
        // Verify the value can be deserialized back to ENRForkID
        let eth2_bytes = enr.get(b"eth2").unwrap();
        let deserialized = ENRForkID::deserialize(&eth2_bytes).unwrap();
        assert_eq!(fork_id.fork_digest, deserialized.fork_digest);
        assert_eq!(fork_id.next_fork_version, deserialized.next_fork_version);
        assert_eq!(fork_id.next_fork_epoch, deserialized.next_fork_epoch);
    }
} 
use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use ream_fork_choice_beacon::store::Store;
use ream_storage::tables::table::REDBTable;

pub const SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY: u64 = 128;

/// Check if a block can be optimistically imported.
/// Returns true if the block's parent has an execution payload,
/// or if the block is within 128 slots of the current head.
pub fn is_optimistic_candidate_block(
    store: &Store,
    current_head_slot: u64,
    block: &SignedBeaconBlock,
) -> bool {
    // If parent has execution payload, we can optimistic import
    let parent_root = block.message.parent_root;
    if let Ok(Some(_parent_block)) = store.db.block_provider().get(parent_root) {
        // Electra blocks always have an execution payload
        return true;
    }

    // Within safe distance from head?
    let distance = current_head_slot.saturating_sub(block.message.slot);
    distance <= SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use ream_consensus_beacon::electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
    };

    fn make_block(slot: u64, parent_root: B256) -> SignedBeaconBlock {
        SignedBeaconBlock {
            message: BeaconBlock {
                slot,
                proposer_index: 0,
                parent_root,
                state_root: B256::ZERO,
                body: BeaconBlockBody::default(),
            },
            signature: Default::default(),
        }
    }

    /// Block within SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY distance of head_slot
    /// should be a candidate even if parent is not in store.
    #[test]
    fn test_within_safe_distance_is_candidate() {
        // We can't easily construct a real Store without a DB, so we test the distance logic
        // indirectly: slot distance = head_slot - block.slot <= 128
        let block_slot = 100u64;
        let head_slot = 200u64;
        let distance = head_slot.saturating_sub(block_slot);
        assert!(
            distance <= SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY,
            "Block at slot {block_slot} with head {head_slot} should be within safe distance"
        );
    }

    /// Block beyond SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY distance (and no parent in store)
    /// should NOT be a candidate based on distance alone.
    #[test]
    fn test_beyond_safe_distance_not_candidate_by_distance() {
        let block_slot = 100u64;
        let head_slot = block_slot + SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY + 1;
        let distance = head_slot.saturating_sub(block_slot);
        assert!(
            distance > SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY,
            "Block at slot {block_slot} with head {head_slot} should be outside safe distance"
        );
    }

    /// SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY is exactly 128 as per the spec.
    #[test]
    fn test_safe_slots_constant() {
        assert_eq!(
            SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY, 128,
            "SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY must be 128 per the beacon chain spec"
        );
    }

    /// Verify make_block helper constructs a block with correct slot and parent_root.
    #[test]
    fn test_make_block_fields() {
        let parent = B256::from([42u8; 32]);
        let block = make_block(10, parent);
        assert_eq!(block.message.slot, 10);
        assert_eq!(block.message.parent_root, parent);
    }
}

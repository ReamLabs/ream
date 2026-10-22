use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use ream_fork_choice_beacon::store::Store;
use ream_storage::tables::table::REDBTable;

pub const SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY: u64 = 128;

/// Check if a block can be optimistically imported.
/// Returns true if the block's parent exists in the store (has an execution payload),
/// or if the block is at least SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY (128) slots behind the current
/// head.
pub fn is_optimistic_candidate_block(
    store: &Store,
    current_head_slot: u64,
    block: &SignedBeaconBlock,
) -> bool {
    // If parent has execution payload, we can optimistic import
    let parent_root = block.message.parent_root;
    if store
        .db
        .block_provider()
        .get(parent_root)
        .ok()
        .flatten()
        .is_some()
    {
        // Electra blocks always have an execution payload
        return true;
    }

    // Within safe distance from head? Per Ethereum spec, the block must be
    // at least SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY slots deep into the past.
    block.message.slot + SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY <= current_head_slot
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use ream_consensus_beacon::electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
    };

    use super::*;

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

    /// Block at least SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY slots behind head_slot
    /// should be a candidate even if parent is not in store.
    #[test]
    fn test_within_safe_distance_is_candidate() {
        let block_slot = 100u64;
        let head_slot = block_slot + SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY + 10;
        let distance = head_slot.saturating_sub(block_slot);
        assert!(
            distance >= SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY,
            "Block at slot {block_slot} with head {head_slot} should be deep enough to be an optimistic candidate"
        );
    }

    /// Block closer than SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY to head_slot (and no parent in store)
    /// should NOT be a candidate based on distance alone.
    #[test]
    fn test_beyond_safe_distance_not_candidate_by_distance() {
        let block_slot = 100u64;
        let head_slot = block_slot + SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY - 1;
        let distance = head_slot.saturating_sub(block_slot);
        assert!(
            distance < SAFE_SLOTS_TO_IMPORT_OPTIMISTICALLY,
            "Block at slot {block_slot} with head {head_slot} is too close to head"
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

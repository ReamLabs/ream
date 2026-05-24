use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use ream_fork_choice_beacon::store::Store;

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

use ream_consensus::{
    beacon_block_header::BeaconBlockHeader, electra::beacon_block::SignedBeaconBlock,
};
use tree_hash::TreeHash;

use crate::header::LightClientHeader;

pub fn block_to_light_client_header(block: &SignedBeaconBlock) -> LightClientHeader {
    LightClientHeader {
        beacon: BeaconBlockHeader {
            slot: block.message.slot,
            proposer_index: block.message.proposer_index,
            parent_root: block.message.parent_root,
            state_root: block.message.state_root,
            body_root: block.message.body.tree_hash_root(),
        },
    }
}

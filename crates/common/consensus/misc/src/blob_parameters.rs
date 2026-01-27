use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::constants::beacon::{ELECTRA_FORK_EPOCH, MAX_BLOBS_PER_BLOCK_ELECTRA};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlobParameters {
    pub epoch: u64,
    pub max_blobs_per_block: u64,
}

pub fn get_blob_parameters(epoch: u64) -> BlobParameters {
    let schedule = [BlobParameters {
        epoch: ELECTRA_FORK_EPOCH,
        max_blobs_per_block: MAX_BLOBS_PER_BLOCK_ELECTRA,
    }];

    schedule
        .iter()
        .rev()
        .find(|entry| epoch >= entry.epoch)
        .cloned()
        .unwrap_or(BlobParameters {
            epoch: ELECTRA_FORK_EPOCH,
            max_blobs_per_block: MAX_BLOBS_PER_BLOCK_ELECTRA,
        })
}

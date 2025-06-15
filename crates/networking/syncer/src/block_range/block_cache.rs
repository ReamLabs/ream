use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use alloy_primitives::B256;
use anyhow::{bail, ensure};
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    electra::beacon_block::SignedBeaconBlock,
};
use ream_network_spec::networks::network_spec;
use ssz::Encode;
use tree_hash::TreeHash;

use super::{MAX_BLOCKS_PER_REQUEST, peer_range_downloader::Range};

/// The size of blobs is 32 * 4096 bytes, which is 131072 bytes or 128 KiB.
const BLOB_SIZE: u64 = 131_072;
/// Assume a default average block size if we have no blocks yet
const DEFAULT_BLOCK_SIZE: u64 = 86_876;
pub const HUNDRED_MEGA_BYTES: u64 = 100 * 1024 * 1024;

pub struct BlockAndBlobBundle {
    pub block: SignedBeaconBlock,
    pub blobs: HashMap<BlobIdentifier, BlobSidecar>,
}

impl BlockAndBlobBundle {
    pub fn new(block: SignedBeaconBlock) -> Self {
        Self {
            block,
            blobs: HashMap::new(),
        }
    }
}

pub struct BlockCache {
    blocks_and_blobs: HashMap<B256, BlockAndBlobBundle>,
    current_cache_size: u64,
    target_size: u64,
    initial_parent_root: B256,
    block_ranges_to_retry: Vec<Range>,
    next_start_slot: u64,
    block_roots_in_progress: HashSet<B256>,
    blob_identifiers_in_progress: HashSet<BlobIdentifier>,
}

impl BlockCache {
    pub fn new(target_size: u64, initial_parent_root: B256, next_start_slot: u64) -> Self {
        Self {
            blocks_and_blobs: HashMap::new(),
            current_cache_size: 0,
            target_size,
            initial_parent_root,
            block_ranges_to_retry: vec![],
            next_start_slot,
            block_roots_in_progress: HashSet::new(),
            blob_identifiers_in_progress: HashSet::new(),
        }
    }

    pub fn add_blocks(&mut self, blocks: Vec<SignedBeaconBlock>) -> anyhow::Result<()> {
        // Ensure that all blocks form a chain
        for (index, block) in blocks.iter().enumerate().rev() {
            if index > 0 {
                ensure!(
                    block.message.parent_root == blocks[index - 1].message.tree_hash_root(),
                    "Block at index {index} has a parent root that does not match the previous block's tree hash root",
                );
            }
        }

        for block in blocks {
            self.current_cache_size += block.as_ssz_bytes().len() as u64;
            self.blocks_and_blobs.insert(
                block.message.tree_hash_root(),
                BlockAndBlobBundle::new(block),
            );
        }

        Ok(())
    }

    pub fn add_blobs(&mut self, blobs: Vec<BlobSidecar>) -> anyhow::Result<()> {
        for blob_sidecar in blobs {
            let block_root = blob_sidecar.signed_block_header.message.tree_hash_root();

            if let Some(bundle) = self.blocks_and_blobs.get_mut(&block_root) {
                bundle.blobs.insert(
                    BlobIdentifier {
                        block_root,
                        index: blob_sidecar.index,
                    },
                    blob_sidecar,
                );
            } else {
                bail!("Block root {block_root} not found in cache, this should be impossible");
            }
        }

        Ok(())
    }

    pub fn extend_block_roots_in_progress(&mut self, block_roots: &[B256]) {
        self.block_roots_in_progress.extend(block_roots);
    }

    pub fn remove_block_roots_in_progress(&mut self, block_roots: &[B256]) {
        for root in block_roots {
            self.block_roots_in_progress.remove(root);
        }
    }

    pub fn extend_blob_identifiers_in_progress(&mut self, blob_identifiers: &[BlobIdentifier]) {
        self.blob_identifiers_in_progress.extend(blob_identifiers);
    }

    pub fn remove_blob_identifiers_in_progress(&mut self, blob_identifiers: &[BlobIdentifier]) {
        for identifier in blob_identifiers {
            self.blob_identifiers_in_progress.remove(identifier);
        }
    }

    pub fn block_count(&self) -> u64 {
        self.blocks_and_blobs.len() as u64
    }

    pub fn blob_count(&self) -> u64 {
        self.blocks_and_blobs
            .values()
            .map(|bundle| bundle.block.message.body.blob_kzg_commitments.len() as u64)
            .sum()
    }

    pub fn downloaded_blob_count(&self) -> u64 {
        self.blocks_and_blobs
            .values()
            .map(|bundle| bundle.blobs.len() as u64)
            .sum()
    }

    pub fn estimated_blocks_to_fetch(&self) -> u64 {
        if self.blocks_and_blobs.is_empty() || self.current_cache_size == 0 {
            return self.target_size.div_ceil(DEFAULT_BLOCK_SIZE);
        }

        let number_of_blocks = self.blocks_and_blobs.len() as u64;
        let average_block_size = self.current_cache_size / number_of_blocks;
        let total_blobs: u64 = self.blob_count();
        let average_blobs_per_block = total_blobs / number_of_blocks;
        let average_total_size_per_block = average_block_size + average_blobs_per_block * BLOB_SIZE;
        let total_cache_size = self.current_cache_size + total_blobs * BLOB_SIZE;
        let remaining_size = self.target_size.saturating_sub(total_cache_size);

        remaining_size.div_ceil(average_total_size_per_block)
    }

    pub fn push_retry_range(&mut self, range: Range) {
        self.block_ranges_to_retry.push(range);
    }

    pub fn data_to_fetch(&mut self, finalized_slot: u64) -> DataToFetch {
        match self.block_ranges_to_retry.pop() {
            Some(range) => return DataToFetch::BlockRange(range),
            None => {
                let estimated_blocks_to_fetch = self.estimated_blocks_to_fetch();
                if estimated_blocks_to_fetch > 0 && self.next_start_slot <= finalized_slot {
                    let blocks_to_fill = estimated_blocks_to_fetch
                        .min(MAX_BLOCKS_PER_REQUEST.min(finalized_slot - self.next_start_slot));
                    self.next_start_slot += blocks_to_fill;
                    return DataToFetch::BlockRange(Range::new(
                        self.next_start_slot,
                        blocks_to_fill,
                    ));
                }
            }
        }

        let mut block_roots_left_to_fetch = self.get_missing_block_roots();
        let missing_block_roots_len = block_roots_left_to_fetch.len();
        block_roots_left_to_fetch.retain(|root| !self.block_roots_in_progress.contains(root));

        let mut blob_identifiers_left_to_fetch = self.get_missing_blob_identifiers();
        let missing_blob_identifiers_len = blob_identifiers_left_to_fetch.len();
        blob_identifiers_left_to_fetch
            .retain(|blob_identifier| !self.blob_identifiers_in_progress.contains(blob_identifier));

        if !block_roots_left_to_fetch.is_empty() {
            return DataToFetch::MissingBlockRoots(block_roots_left_to_fetch);
        }

        if !blob_identifiers_left_to_fetch.is_empty() {
            return DataToFetch::MissingBlobIdentifiers(blob_identifiers_left_to_fetch);
        }

        if missing_block_roots_len > 0 || missing_blob_identifiers_len > 0 {
            return DataToFetch::DownloadsInProgress;
        }

        DataToFetch::Finished
    }

    /// Return the blocks in sorted order to be processed.
    pub fn get_blocks_and_blobs(mut self) -> anyhow::Result<Vec<BlockAndBlobBundle>> {
        let missing_block_roots = self.get_missing_block_roots();
        if !missing_block_roots.is_empty() {
            bail!("Missing block roots: {}", missing_block_roots.len());
        } else {
            let mut blocks_and_blobs = self
                .blocks_and_blobs
                .drain()
                .map(|(_, block)| block)
                .collect::<Vec<_>>();
            blocks_and_blobs.sort_by_key(|block| block.block.message.slot);
            Ok(blocks_and_blobs)
        }
    }

    fn get_missing_block_roots(&self) -> Vec<B256> {
        let mut missing_roots = Vec::new();
        for block in self.blocks_and_blobs.values() {
            if !self
                .blocks_and_blobs
                .contains_key(&block.block.message.parent_root)
                && block.block.message.parent_root != self.initial_parent_root
            {
                missing_roots.push(block.block.message.parent_root);
            }
        }
        missing_roots
    }

    fn get_missing_blob_identifiers(&self) -> Vec<BlobIdentifier> {
        let slot_17_days_ago = network_spec().slot_n_days_ago(17);
        let mut missing_roots = Vec::new();
        for block in self.blocks_and_blobs.values() {
            if block.block.message.slot < slot_17_days_ago {
                continue;
            }

            let block_root = block.block.message.tree_hash_root();
            for index in 0..block.block.message.body.blob_kzg_commitments.len() {
                let blob_identifier = BlobIdentifier {
                    block_root,
                    index: index as u64,
                };
                if block.blobs.contains_key(&blob_identifier) {
                    continue;
                }
                missing_roots.push(blob_identifier);
            }
        }
        missing_roots
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataToFetch {
    BlockRange(Range),
    MissingBlockRoots(Vec<B256>),
    MissingBlobIdentifiers(Vec<BlobIdentifier>),
    DownloadsInProgress,
    Finished,
}

impl Display for DataToFetch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataToFetch::BlockRange(range) => write!(f, "BlockRange({range:?})"),
            DataToFetch::MissingBlockRoots(roots) => {
                write!(f, "MissingBlockRoots({})", roots.len())
            }
            DataToFetch::MissingBlobIdentifiers(identifiers) => {
                write!(f, "MissingBlobIdentifiers({})", identifiers.len())
            }
            DataToFetch::DownloadsInProgress => write!(f, "DownloadsInProgress"),
            DataToFetch::Finished => write!(f, "Finished"),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use ream_bls::BLSSignature;
    use ream_consensus::electra::beacon_block::BeaconBlock;

    use super::*;

    #[test]
    fn test_estimated_blocks_to_fetch() {
        let block_count = 2;
        let mut blocks = vec![];
        let mut current_size = 0;
        let mut parent_root = B256::ZERO;
        for i in 1..=2 {
            let block = SignedBeaconBlock {
                message: BeaconBlock {
                    slot: i,
                    parent_root,
                    ..Default::default()
                },
                signature: BLSSignature::infinity(),
            };
            parent_root = block.message.tree_hash_root();
            current_size += block.as_ssz_bytes().len() as u64;
            blocks.push(block);
        }

        let mut cache = BlockCache::new(current_size / block_count * 10, B256::ZERO, 1);

        cache.add_blocks(blocks).unwrap();

        assert_eq!(cache.current_cache_size, current_size);
        assert_eq!(cache.estimated_blocks_to_fetch(), 8);
    }

    #[test]
    fn test_empty_cache_estimated_blocks() {
        let cache = BlockCache::new(HUNDRED_MEGA_BYTES, B256::ZERO, 1);
        assert_eq!(
            cache.estimated_blocks_to_fetch(),
            HUNDRED_MEGA_BYTES.div_ceil(DEFAULT_BLOCK_SIZE)
        );
    }
}

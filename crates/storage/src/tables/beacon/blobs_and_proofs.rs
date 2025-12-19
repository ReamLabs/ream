use std::{
    fs::{File, read_dir, remove_file},
    io::{Read, Write},
    path::PathBuf,
};

use alloy_primitives::B256;
use ream_consensus_beacon::blob_sidecar::BlobIdentifier;
use ream_execution_rpc_types::get_blobs::BlobAndProofV1;
use snap::raw::{Decoder, Encoder};
use ssz::{Decode, Encode};
use tracing::{debug, error, info};

use crate::{errors::StoreError, tables::table::CustomTable};

pub(crate) const BLOB_FOLDER_NAME: &str = "beacon_blobs";

pub struct BlobsAndProofsTable {
    pub data_dir: PathBuf,
}

impl BlobsAndProofsTable {
    fn blob_file_path(&self, blob_identifier: &BlobIdentifier) -> PathBuf {
        self.data_dir.join(BLOB_FOLDER_NAME).join(format!(
            "{}_{}.ssz_snappy",
            blob_identifier.block_root, blob_identifier.index
        ))
    }

    /// Prune blobs older than the specified slot by removing their associated block roots
    pub fn prune_old_blobs(
        &self,
        blocks_to_retain: &std::collections::HashSet<B256>,
    ) -> Result<usize, StoreError> {
        let blob_dir = self.data_dir.join(BLOB_FOLDER_NAME);

        if !blob_dir.exists() {
            return Ok(0);
        }

        let mut pruned_count = 0;

        let entries = read_dir(&blob_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // Extract block root from filename (format: {block_root}_{index}.ssz_snappy)
            if let Some(filename) = path.file_name().and_then(|n| n.to_str())
                && let Some(block_root_str) = filename.split('_').next()
                && let Ok(block_root) = block_root_str.parse::<B256>()
                // If the block root is not in the retention set, remove the file
                && !blocks_to_retain.contains(&block_root)
            {
                match remove_file(&path) {
                    Ok(_) => {
                        pruned_count += 1;
                        debug!("Pruned blob file: {:?}", path);
                    }
                    Err(err) => {
                        error!("Failed to remove blob file {:?}: {}", path, err);
                    }
                }
            }
        }

        if pruned_count > 0 {
            info!("Pruned {} old blob files", pruned_count);
        }

        Ok(pruned_count)
    }
}

impl CustomTable for BlobsAndProofsTable {
    type Key = BlobIdentifier;

    type Value = BlobAndProofV1;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let file_path = self.blob_file_path(&key);

        if !file_path.exists() {
            return Ok(None);
        }

        let mut bytes = vec![];
        let mut file = File::open(file_path)?;
        file.read_to_end(&mut bytes)?;
        let mut decoder = Decoder::new();
        let snappy_decoding = decoder.decompress_vec(&bytes)?;

        Ok(Some(BlobAndProofV1::from_ssz_bytes(&snappy_decoding)?))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let file_path = self.blob_file_path(&key);
        let mut encoder = Encoder::new();
        let snappy_encoding = encoder.compress_vec(&value.as_ssz_bytes())?;
        let mut file = File::create(file_path)?;
        file.write_all(&snappy_encoding)?;

        Ok(())
    }

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let blob = self.get(key)?;
        remove_file(self.blob_file_path(&key))?;
        Ok(blob)
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs};

    use alloy_primitives::B256;
    use ream_consensus_beacon::blob_sidecar::BlobIdentifier;
    use ream_execution_rpc_types::get_blobs::BlobAndProofV1;
    use tempdir::TempDir;

    use crate::{
        errors::StoreError,
        tables::{
            beacon::blobs_and_proofs::{BLOB_FOLDER_NAME, BlobsAndProofsTable},
            table::CustomTable,
        },
    };

    #[test]
    fn test_retrieving_blob() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_retrieving_blob")?;

        let blob_dir = tmp_dir.path().to_path_buf().join(BLOB_FOLDER_NAME);
        fs::create_dir_all(&blob_dir)?;

        let table = BlobsAndProofsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        let key = BlobIdentifier::default();
        let value = BlobAndProofV1::default();

        table.insert(key, value.clone())?;

        let result = table.get(key)?;

        assert_eq!(result, Some(value));

        Ok(())
    }

    #[test]
    fn test_no_blobs_available() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_no_blobs_available")?;

        let blob_dir = tmp_dir.path().to_path_buf().join(BLOB_FOLDER_NAME);
        fs::create_dir_all(&blob_dir)?;

        let table = BlobsAndProofsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        let key = BlobIdentifier::default();

        let result = table.get(key)?;

        assert_eq!(result, None);

        Ok(())
    }

    #[test]
    fn test_prune_old_blobs() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_prune_old_blobs")?;

        let blob_dir = tmp_dir.path().to_path_buf().join(BLOB_FOLDER_NAME);
        fs::create_dir_all(&blob_dir)?;

        let table = BlobsAndProofsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        // Create some test blobs with different block roots
        let block_root_1 = B256::from([1u8; 32]);
        let block_root_2 = B256::from([2u8; 32]);
        let block_root_3 = B256::from([3u8; 32]);

        let blob_1 = BlobIdentifier::new(block_root_1, 0);
        let blob_2 = BlobIdentifier::new(block_root_2, 0);
        let blob_3 = BlobIdentifier::new(block_root_3, 0);

        let value = BlobAndProofV1::default();

        // Insert three blobs
        table.insert(blob_1, value.clone())?;
        table.insert(blob_2, value.clone())?;
        table.insert(blob_3, value.clone())?;

        // Verify all three blobs exist
        assert!(table.get(blob_1)?.is_some());
        assert!(table.get(blob_2)?.is_some());
        assert!(table.get(blob_3)?.is_some());

        // Create a retention set that only includes block_root_2 and block_root_3
        let mut blocks_to_retain = HashSet::new();
        blocks_to_retain.insert(block_root_2);
        blocks_to_retain.insert(block_root_3);

        // Prune old blobs
        let pruned_count = table.prune_old_blobs(&blocks_to_retain)?;

        // Should have pruned 1 blob (blob_1)
        assert_eq!(pruned_count, 1);

        // Verify blob_1 is gone but blob_2 and blob_3 remain
        assert!(table.get(blob_1)?.is_none());
        assert!(table.get(blob_2)?.is_some());
        assert!(table.get(blob_3)?.is_some());

        Ok(())
    }

    #[test]
    fn test_prune_with_empty_retention_set() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_prune_with_empty_retention_set")?;

        let blob_dir = tmp_dir.path().to_path_buf().join(BLOB_FOLDER_NAME);
        fs::create_dir_all(&blob_dir)?;

        let table = BlobsAndProofsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        // Create a test blob
        let key = BlobIdentifier::default();
        let value = BlobAndProofV1::default();

        table.insert(key, value.clone())?;

        // Verify blob exists
        assert!(table.get(key)?.is_some());

        // Prune with empty retention set (should remove all blobs)
        let blocks_to_retain = HashSet::new();
        let pruned_count = table.prune_old_blobs(&blocks_to_retain)?;

        // Should have pruned 1 blob
        assert_eq!(pruned_count, 1);

        // Verify blob is gone
        assert!(table.get(key)?.is_none());

        Ok(())
    }
}

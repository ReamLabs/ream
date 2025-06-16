use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use ream_consensus::{
    blob_sidecar::BlobIdentifier, execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
use snap::raw::{Decoder, Encoder};
use ssz::{Decode, Encode};

use super::Table;
use crate::errors::StoreError;

pub const BLOB_FOLDER_NAME: &str = "blobs";

pub struct BlobsAndProofsTable {
    pub data_dir: PathBuf,
}

impl Table for BlobsAndProofsTable {
    type Key = BlobIdentifier;

    type Value = BlobAndProofV1;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let blob_dir = self.data_dir.join(BLOB_FOLDER_NAME);
        let file_path = blob_dir.join(blob_filename(&key));

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
        let blob_dir = self.data_dir.join(BLOB_FOLDER_NAME);

        let file_path = blob_dir.join(blob_filename(&key));

        let mut encoder = Encoder::new();
        let snappy_encoding = encoder.compress_vec(&value.as_ssz_bytes())?;
        let mut file = File::create(file_path)?;
        file.write_all(&snappy_encoding)?;

        Ok(())
    }
}

pub fn blob_filename(blob_identifier: &BlobIdentifier) -> String {
    format!(
        "{}_{}.ssz_snappy",
        blob_identifier.block_root, blob_identifier.index
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ream_consensus::{
        blob_sidecar::BlobIdentifier, execution_engine::rpc_types::get_blobs::BlobAndProofV1,
    };
    use tempdir::TempDir;

    use crate::{
        errors::StoreError,
        tables::{
            Table,
            blobs_and_proofs::{BLOB_FOLDER_NAME, BlobsAndProofsTable},
        },
    };

    #[test]
    fn test_blobs() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_blobs")?;

        let blob_dir = tmp_dir.path().to_path_buf().join(BLOB_FOLDER_NAME);
        fs::create_dir_all(&blob_dir)?;

        let table = BlobsAndProofsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        let key = BlobIdentifier::default();
        let value = BlobAndProofV1::default();

        table.insert(key.clone(), value.clone())?;

        let result = table.get(key)?;

        assert_eq!(result, Some(value));

        Ok(())
    }

    #[test]
    fn test_no_blobs_available() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_blobs")?;

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
}

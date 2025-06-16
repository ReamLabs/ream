use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use ream_consensus::{
    blob_sidecar::BlobIdentifier, execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
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
        let file_name = format!("{} {}", key.block_root, key.index);
        let file_path = blob_dir.join(file_name);
        let mut byte = vec![];
        let mut file = File::open(file_path)?;
        file.read_to_end(&mut byte)?;

        Ok(Some(BlobAndProofV1::from_ssz_bytes(&byte)?))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let blob_dir = self.data_dir.join(BLOB_FOLDER_NAME);

        let file_name = format!("{} {}", key.block_root, key.index);
        let file_path = blob_dir.join(file_name);

        let ssz_encoding = value.as_ssz_bytes();
        let mut file = File::create(file_path)?;
        file.write_all(&ssz_encoding)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use alloy_primitives::FixedBytes;
    use ream_consensus::{
        blob_sidecar::BlobIdentifier,
        execution_engine::rpc_types::get_blobs::{Blob, BlobAndProofV1},
    };
    use ssz_types::{FixedVector, typenum::U131072};
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

        let block_root = [0u8; 32];
        let index = u64::default();

        let key = BlobIdentifier {
            block_root: block_root.into(),
            index,
        };

        let bytes = FixedVector::<u8, U131072>::default();
        let blob = Blob { inner: bytes };

        let proof = FixedBytes::<48>::default();

        let value = BlobAndProofV1 { blob, proof };

        table.insert(key.clone(), value.clone())?;

        let _result = table.get(key)?;

        Ok(())
    }
}

use std::{
    fs::{self, File, remove_file},
    io::{Read, Write},
    path::PathBuf,
};

use alloy_primitives::B256;
use anyhow::{Context, Result};
use ream_consensus_beacon::data_column_sidecar::DataColumnSidecar;
use snap::raw::{Decoder, Encoder};
use ssz::{Decode, Encode};

const COLUMN_FOLDER_NAME: &str = "da_columns";

pub struct DaColumnStore {
    data_dir: PathBuf,
}

impl DaColumnStore {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(data_dir.join(COLUMN_FOLDER_NAME))?;
        Ok(Self { data_dir })
    }

    fn file_path(&self, block_root: B256, index: u64) -> PathBuf {
        self.data_dir
            .join(COLUMN_FOLDER_NAME)
            .join(format!("{block_root}_{index}.ssz_snappy"))
    }

    pub fn get(&self, block_root: B256, index: u64) -> Result<Option<DataColumnSidecar>> {
        let path = self.file_path(block_root, index);
        if !path.exists() {
            return Ok(None);
        }

        let mut bytes = Vec::new();
        File::open(&path)
            .context("opening column file")?
            .read_to_end(&mut bytes)
            .context("reading column file")?;

        let decoded = Decoder::new()
            .decompress_vec(&bytes)
            .context("snappy decompression")?;

        DataColumnSidecar::from_ssz_bytes(&decoded)
            .map(Some)
            .map_err(|e| anyhow::anyhow!("SSZ decode: {e:?}"))
    }

    pub fn insert(&self, block_root: B256, sidecar: DataColumnSidecar) -> Result<()> {
        let path = self.file_path(block_root, sidecar.index);
        let encoded = Encoder::new()
            .compress_vec(&sidecar.as_ssz_bytes())
            .context("snappy compression")?;

        File::create(&path)
            .context("creating column file")?
            .write_all(&encoded)
            .context("writing column file")?;

        Ok(())
    }

    pub fn remove(&self, block_root: B256, index: u64) -> Result<()> {
        let path = self.file_path(block_root, index);
        if path.exists() {
            remove_file(&path).context("removing column file")?;
        }
        Ok(())
    }

    pub fn count(&self, block_root: B256) -> usize {
        let dir = self.data_dir.join(COLUMN_FOLDER_NAME);
        let prefix = format!("{block_root}_");
        fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn remove_all_for_block(&self, block_root: B256) -> Result<usize> {
        let dir = self.data_dir.join(COLUMN_FOLDER_NAME);
        let prefix = format!("{block_root}_");
        let mut removed = 0;

        for entry in fs::read_dir(&dir).context("reading column dir")? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                remove_file(entry.path())?;
                removed += 1;
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ream_consensus_misc::beacon_block_header::SignedBeaconBlockHeader;
    use ssz_types::{FixedVector, VariableList};
    use tempdir::TempDir;

    fn make_sidecar(index: u64) -> DataColumnSidecar {
        let mut sidecar = DataColumnSidecar {
            index: 0,
            column: VariableList::empty(),
            kzg_commitments: VariableList::empty(),
            kzg_proofs: VariableList::empty(),
            signed_block_header: SignedBeaconBlockHeader::default(),
            kzg_commitments_inclusion_proof: FixedVector::default(),
        };
        sidecar.index = index;
        sidecar
    }

    #[test]
    fn test_insert_and_get() {
        let tmp = TempDir::new("da_col").unwrap();
        let store = DaColumnStore::new(tmp.path().to_path_buf()).unwrap();
        let root = B256::ZERO;

        store.insert(root, make_sidecar(0)).unwrap();
        assert!(store.get(root, 0).unwrap().is_some());
        assert!(store.get(root, 1).unwrap().is_none());
    }

    #[test]
    fn test_count() {
        let tmp = TempDir::new("da_count").unwrap();
        let store = DaColumnStore::new(tmp.path().to_path_buf()).unwrap();
        let root = B256::ZERO;

        assert_eq!(store.count(root), 0);
        for i in 0..5 {
            store.insert(root, make_sidecar(i)).unwrap();
        }
        assert_eq!(store.count(root), 5);
    }

    #[test]
    fn test_remove_all_for_block() {
        let tmp = TempDir::new("da_remove").unwrap();
        let store = DaColumnStore::new(tmp.path().to_path_buf()).unwrap();
        let root = B256::ZERO;

        for i in 0..5 {
            store.insert(root, make_sidecar(i)).unwrap();
        }
        let removed = store.remove_all_for_block(root).unwrap();
        assert_eq!(removed, 5);
        assert_eq!(store.count(root), 0);
    }
}

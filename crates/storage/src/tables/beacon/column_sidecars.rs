use std::{
    fs::{File, remove_file},
    io::{Read, Write},
    path::PathBuf,
};

use ream_consensus_beacon::data_column_sidecar::{ColumnIdentifier, DataColumnSidecar};
use snap::raw::{Decoder, Encoder};
use ssz::{Decode, Encode};

use crate::{errors::StoreError, tables::table::CustomTable};

pub(crate) const COLUMN_FOLDER_NAME: &str = "beacon_columns";

pub struct ColumnSidecarsTable {
    pub data_dir: PathBuf,
}

impl ColumnSidecarsTable {
    fn column_file_path(&self, column_identifier: &ColumnIdentifier) -> PathBuf {
        self.data_dir.join(COLUMN_FOLDER_NAME).join(format!(
            "{}_{}.ssz_snappy",
            column_identifier.block_root, column_identifier.index
        ))
    }
}

impl CustomTable for ColumnSidecarsTable {
    type Key = ColumnIdentifier;
    type Value = DataColumnSidecar;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let file_path = self.column_file_path(&key);

        if !file_path.exists() {
            return Ok(None);
        }

        let mut bytes = vec![];
        let mut file = File::open(file_path)?;
        file.read_to_end(&mut bytes)?;
        let mut decoder = Decoder::new();
        let snappy_decoding = decoder.decompress_vec(&bytes)?;

        Ok(Some(DataColumnSidecar::from_ssz_bytes(&snappy_decoding)?))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let file_path = self.column_file_path(&key);
        let mut encoder = Encoder::new();
        let snappy_encoding = encoder.compress_vec(&value.as_ssz_bytes())?;
        let parent = file_path.parent().ok_or_else(|| {
            std::io::Error::other(format!(
                "Column sidecar path has no parent: {}",
                file_path.display()
            ))
        })?;
        let mut temp_file = tempfile::NamedTempFile::new_in(parent)?;
        temp_file.write_all(&snappy_encoding)?;
        temp_file.persist(file_path).map_err(|err| err.error)?;

        Ok(())
    }

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let column = self.get(key)?;
        remove_file(self.column_file_path(&key))?;
        Ok(column)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ream_consensus_beacon::data_column_sidecar::ColumnIdentifier;
    use tempdir::TempDir;

    use crate::{
        errors::StoreError,
        tables::{
            beacon::column_sidecars::{COLUMN_FOLDER_NAME, ColumnSidecarsTable},
            table::CustomTable,
        },
    };

    #[test]
    fn test_column_identifier_default() -> Result<(), StoreError> {
        let tmp_dir = TempDir::new("test_column_sidecar")?;

        let column_dir = tmp_dir.path().to_path_buf().join(COLUMN_FOLDER_NAME);
        fs::create_dir_all(&column_dir)?;

        let table = ColumnSidecarsTable {
            data_dir: tmp_dir.path().to_path_buf(),
        };

        let key = ColumnIdentifier::default();

        let result = table.get(key)?;

        assert_eq!(result, None);

        Ok(())
    }
}

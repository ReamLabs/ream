use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use redb::{Builder, Database};

use crate::{dir, errors::StoreError, tables::beacon_state::BeaconStateTable};

pub const APP_NAME: &str = "ream";

pub const REDB_FILE: &str = "ream.redb";

/// The size of the cache for the database
///
/// 1 GiB
pub const REDB_CACHE_SIZE: usize = 1_024 * 1_024 * 1_024;

#[derive(Clone, Debug)]
pub struct ReamDB {
    pub db: Arc<Database>,
}

#[allow(clippy::result_large_err)]
impl ReamDB {
    pub fn new(data_dir: Option<PathBuf>, ephemeral: bool) -> Result<Self, StoreError> {
        let ream_dir =
            dir::setup_data_dir(APP_NAME, data_dir, ephemeral).map_err(StoreError::Io)?;

        let ream_file = ream_dir.join(REDB_FILE);

        let db = Builder::new()
            .set_cache_size(REDB_CACHE_SIZE)
            .create(&ream_file)
            .map_err(|err| StoreError::Database(err.into()))?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn beacon_state_provider(&self) -> BeaconStateTable {
        BeaconStateTable {
            db: self.db.clone(),
        }
    }
}

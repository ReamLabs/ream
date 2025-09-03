use std::{fs, io, path::PathBuf, sync::Arc};

use anyhow::Result;
use redb::{Builder, Database};
use tracing::info;

use crate::errors::StoreError;

pub const REDB_FILE: &str = "lean.redb";

/// The size of the cache for the database
///
/// 1 GiB
pub const REDB_CACHE_SIZE: usize = 1_024 * 1_024 * 1_024;

#[derive(Clone, Debug)]
pub struct ReamLeanDB {
    pub db: Arc<Database>,
    pub data_dir: PathBuf,
}

impl ReamLeanDB {
    pub fn new(data_dir: PathBuf) -> Result<Self, StoreError> {
        let db = Builder::new()
            .set_cache_size(REDB_CACHE_SIZE)
            .create(data_dir.join(REDB_FILE))?;

        Ok(Self {
            db: Arc::new(db),
            data_dir,
        })
    }
}

pub fn reset_db(db_path: PathBuf) -> anyhow::Result<()> {
    if fs::read_dir(&db_path)?.next().is_none() {
        info!("Data directory at {db_path:?} is already empty.");
        return Ok(());
    }

    info!(
        "Are you sure you want to clear the contents of the data directory at {db_path:?}? (y/n):"
    );
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") {
        for entry in fs::read_dir(&db_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
        info!("Database contents cleared successfully.");
    } else {
        info!("Operation canceled by user.");
    }
    Ok(())
}

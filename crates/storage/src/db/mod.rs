pub mod beacon;
pub mod lean;

use std::{fs, io, path::PathBuf, sync::Arc};

use anyhow::Result;
use beacon::BeaconDB;
use lean::LeanDB;
use redb::{Builder, Database};
use tracing::info;

use crate::{
    errors::StoreError,
    tables::{
        beacon::{
            beacon_block::BEACON_BLOCK_TABLE, beacon_state::BEACON_STATE_TABLE,
            blobs_and_proofs::BLOB_FOLDER_NAME, block_timeliness::BLOCK_TIMELINESS_TABLE,
            checkpoint_states::CHECKPOINT_STATES_TABLE,
            equivocating_indices::EQUIVOCATING_INDICES_FIELD,
            finalized_checkpoint::FINALIZED_CHECKPOINT_FIELD, genesis_time::GENESIS_TIME_FIELD,
            justified_checkpoint::JUSTIFIED_CHECKPOINT_FIELD,
            latest_messages::LATEST_MESSAGES_TABLE,
            parent_root_index::PARENT_ROOT_INDEX_MULTIMAP_TABLE,
            proposer_boost_root::PROPOSER_BOOST_ROOT_FIELD, slot_index::SLOT_INDEX_TABLE,
            state_root_index::STATE_ROOT_INDEX_TABLE, time::TIME_FIELD,
            unrealized_finalized_checkpoint::UNREALIZED_FINALIZED_CHECKPOINT_FIELD,
            unrealized_justifications::UNREALIZED_JUSTIFICATIONS_TABLE,
            unrealized_justified_checkpoint::UNREALIZED_JUSTIFED_CHECKPOINT_FIELD,
        },
        lean::{
            lean_block::LEAN_BLOCK_TABLE, lean_state::LEAN_STATE_TABLE,
            slot_index::LEAN_SLOT_INDEX_TABLE, state_root_index::LEAN_STATE_ROOT_INDEX_TABLE,
        },
    },
};

pub const REDB_FILE: &str = "ream.redb";

/// The size of the cache for the database
///
/// 1 GiB
pub const REDB_CACHE_SIZE: usize = 1_024 * 1_024 * 1_024;

#[derive(Clone, Debug)]
pub struct ReamDB {
    db: Arc<Database>,
    data_dir: PathBuf,
}

impl ReamDB {
    pub fn new(data_dir: PathBuf) -> Result<Self, StoreError> {
        let db = Builder::new()
            .set_cache_size(REDB_CACHE_SIZE)
            .create(data_dir.join(REDB_FILE))?;

        Ok(ReamDB {
            db: Arc::new(db),
            data_dir,
        })
    }

    pub fn init_beacon_db(&self) -> Result<BeaconDB, StoreError> {
        let write_txn = self.db.begin_write()?;

        write_txn.open_table(BEACON_BLOCK_TABLE)?;
        write_txn.open_table(BEACON_STATE_TABLE)?;
        write_txn.open_table(BLOCK_TIMELINESS_TABLE)?;
        write_txn.open_table(CHECKPOINT_STATES_TABLE)?;
        write_txn.open_table(EQUIVOCATING_INDICES_FIELD)?;
        write_txn.open_table(FINALIZED_CHECKPOINT_FIELD)?;
        write_txn.open_table(GENESIS_TIME_FIELD)?;
        write_txn.open_table(JUSTIFIED_CHECKPOINT_FIELD)?;
        write_txn.open_table(LATEST_MESSAGES_TABLE)?;
        write_txn.open_multimap_table(PARENT_ROOT_INDEX_MULTIMAP_TABLE)?;
        write_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        write_txn.open_table(SLOT_INDEX_TABLE)?;
        write_txn.open_table(STATE_ROOT_INDEX_TABLE)?;
        write_txn.open_table(TIME_FIELD)?;
        write_txn.open_table(UNREALIZED_FINALIZED_CHECKPOINT_FIELD)?;
        write_txn.open_table(UNREALIZED_JUSTIFICATIONS_TABLE)?;
        write_txn.open_table(UNREALIZED_JUSTIFED_CHECKPOINT_FIELD)?;
        write_txn.commit()?;

        fs::create_dir_all(self.data_dir.join(BLOB_FOLDER_NAME))?;

        Ok(BeaconDB {
            db: self.db.clone(),
            data_dir: self.data_dir.clone(),
        })
    }

    pub fn init_lean_db(&self) -> Result<LeanDB, StoreError> {
        let write_txn = self.db.begin_write()?;

        write_txn.open_table(LEAN_BLOCK_TABLE)?;
        write_txn.open_table(LEAN_STATE_TABLE)?;
        write_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        write_txn.open_table(LEAN_STATE_ROOT_INDEX_TABLE)?;
        write_txn.commit()?;

        Ok(LeanDB {
            db: self.db.clone(),
        })
    }
}

pub fn reset_db(db_path: &PathBuf) -> anyhow::Result<()> {
    if fs::read_dir(db_path)?.next().is_none() {
        info!("Data directory at {db_path:?} is already empty.");
        return Ok(());
    }

    info!(
        "Are you sure you want to clear the contents of the data directory at {db_path:?}? (y/n):"
    );
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().eq_ignore_ascii_case("y") {
        for entry in fs::read_dir(db_path)? {
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

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
            beacon_block::BeaconBlockTable, beacon_state::BeaconStateTable,
            blobs_and_proofs::BLOB_FOLDER_NAME, block_timeliness::BlockTimelinessTable,
            checkpoint_states::CheckpointStatesTable,
            equivocating_indices::EQUIVOCATING_INDICES_FIELD,
            finalized_checkpoint::FINALIZED_CHECKPOINT_FIELD, genesis_time::GENESIS_TIME_FIELD,
            justified_checkpoint::JUSTIFIED_CHECKPOINT_FIELD, latest_messages::LatestMessagesTable,
            parent_root_index::PARENT_ROOT_INDEX_MULTIMAP_TABLE,
            proposer_boost_root::PROPOSER_BOOST_ROOT_FIELD, slot_index::BeaconSlotIndexTable,
            state_root_index::BeaconStateRootIndexTable, time::TIME_FIELD,
            unrealized_finalized_checkpoint::UNREALIZED_FINALIZED_CHECKPOINT_FIELD,
            unrealized_justifications::UnrealizedJustificationsTable,
            unrealized_justified_checkpoint::UNREALIZED_JUSTIFED_CHECKPOINT_FIELD,
        },
        lean::{
            latest_finalized::LATEST_FINALIZED_FIELD, latest_justified::LATEST_JUSTIFIED_FIELD,
            lean_block::LeanBlockTable, lean_head::LEAN_HEAD_FIELD,
            lean_safe_target::LEAN_SAFE_TARGET_FIELD, lean_state::LeanStateTable,
            lean_time::LEAN_TIME_FIELD, slot_index::LeanSlotIndexTable,
            state_root_index::LeanStateRootIndexTable,
        },
        table::REDBTable,
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

        write_txn.open_table(BeaconBlockTable::TABLE_DEFINITION)?;
        write_txn.open_table(BeaconStateTable::TABLE_DEFINITION)?;
        write_txn.open_table(BlockTimelinessTable::TABLE_DEFINITION)?;
        write_txn.open_table(CheckpointStatesTable::TABLE_DEFINITION)?;
        write_txn.open_table(EQUIVOCATING_INDICES_FIELD)?;
        write_txn.open_table(FINALIZED_CHECKPOINT_FIELD)?;
        write_txn.open_table(GENESIS_TIME_FIELD)?;
        write_txn.open_table(JUSTIFIED_CHECKPOINT_FIELD)?;
        write_txn.open_table(LatestMessagesTable::TABLE_DEFINITION)?;
        write_txn.open_multimap_table(PARENT_ROOT_INDEX_MULTIMAP_TABLE)?;
        write_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        write_txn.open_table(BeaconSlotIndexTable::TABLE_DEFINITION)?;
        write_txn.open_table(BeaconStateRootIndexTable::TABLE_DEFINITION)?;
        write_txn.open_table(TIME_FIELD)?;
        write_txn.open_table(UNREALIZED_FINALIZED_CHECKPOINT_FIELD)?;
        write_txn.open_table(UnrealizedJustificationsTable::TABLE_DEFINITION)?;
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

        write_txn.open_table(LATEST_FINALIZED_FIELD)?;
        write_txn.open_table(LATEST_JUSTIFIED_FIELD)?;
        write_txn.open_table(LeanBlockTable::TABLE_DEFINITION)?;
        write_txn.open_table(LeanStateTable::TABLE_DEFINITION)?;
        write_txn.open_table(LeanSlotIndexTable::TABLE_DEFINITION)?;
        write_txn.open_table(LeanStateRootIndexTable::TABLE_DEFINITION)?;
        write_txn.open_table(LEAN_TIME_FIELD)?;
        write_txn.open_table(LEAN_HEAD_FIELD)?;
        write_txn.open_table(LEAN_SAFE_TARGET_FIELD)?;
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

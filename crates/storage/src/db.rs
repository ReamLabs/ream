use std::{fs, io, path::PathBuf, sync::Arc};

use anyhow::Result;
use redb::{Builder, Database};
use tracing::info;

use crate::{
    errors::StoreError,
    tables::{
        beacon_block::{BEACON_BLOCK_TABLE, BeaconBlockTable},
        beacon_state::{BEACON_STATE_TABLE, BeaconStateTable},
        blobs_and_proofs::{BLOBS_AND_PROOFS_TABLE, BlobsAndProofsTable},
        block_timeliness::{BLOCK_TIMELINESS_TABLE, BlockTimelinessTable},
        checkpoint_states::{CHECKPOINT_STATES_TABLE, CheckpointStatesTable},
        equivocating_indices::{EQUIVOCATING_INDICES_FIELD, EquivocatingIndicesField},
        finalized_checkpoint::{FINALIZED_CHECKPOINT_FIELD, FinalizedCheckpointField},
        genesis_time::{GENESIS_TIME_FIELD, GenesisTimeField},
        justified_checkpoint::{JUSTIFIED_CHECKPOINT_FIELD, JustifiedCheckpointField},
        latest_messages::{LATEST_MESSAGES_TABLE, LatestMessagesTable},
        parent_root_index::{PARENT_ROOT_INDEX_MULTIMAP_TABLE, ParentRootIndexMultimapTable},
        proposer_boost_root::{PROPOSER_BOOST_ROOT_FIELD, ProposerBoostRootField},
        slot_index::{SLOT_INDEX_TABLE, SlotIndexTable},
        state_root_index::{STATE_ROOT_INDEX_TABLE, StateRootIndexTable},
        time::{TIME_FIELD, TimeField},
        unrealized_finalized_checkpoint::{
            UNREALIZED_FINALIZED_CHECKPOINT_FIELD, UnrealizedFinalizedCheckpointField,
        },
        unrealized_justifications::{
            UNREALIZED_JUSTIFICATIONS_TABLE, UnrealizedJustificationsTable,
        },
        unrealized_justified_checkpoint::{
            UNREALIZED_JUSTIFED_CHECKPOINT_FIELD, UnrealizedJustifiedCheckpointField,
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
    pub db: Arc<Database>,
}

impl ReamDB {
    pub fn new(ream_dir: PathBuf) -> Result<Self, StoreError> {
        let ream_file = ream_dir.join(REDB_FILE);

        let db = Builder::new()
            .set_cache_size(REDB_CACHE_SIZE)
            .create(&ream_file)?;

        let write_txn = db.begin_write()?;
        write_txn.open_table(BEACON_BLOCK_TABLE)?;
        write_txn.open_table(BEACON_STATE_TABLE)?;
        write_txn.open_table(BLOBS_AND_PROOFS_TABLE)?;
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

        Ok(Self { db: Arc::new(db) })
    }

    pub fn beacon_block_provider(&self) -> BeaconBlockTable {
        BeaconBlockTable {
            db: self.db.clone(),
        }
    }

    pub fn beacon_state_provider(&self) -> BeaconStateTable {
        BeaconStateTable {
            db: self.db.clone(),
        }
    }

    pub fn blobs_and_proofs_provider(&self) -> BlobsAndProofsTable {
        BlobsAndProofsTable {
            db: self.db.clone(),
        }
    }

    pub fn block_timeliness_provider(&self) -> BlockTimelinessTable {
        BlockTimelinessTable {
            db: self.db.clone(),
        }
    }

    pub fn checkpoint_states_provider(&self) -> CheckpointStatesTable {
        CheckpointStatesTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_messages_provider(&self) -> LatestMessagesTable {
        LatestMessagesTable {
            db: self.db.clone(),
        }
    }

    pub fn unrealized_justifications_provider(&self) -> UnrealizedJustificationsTable {
        UnrealizedJustificationsTable {
            db: self.db.clone(),
        }
    }

    pub fn parent_root_index_multimap_provider(&self) -> ParentRootIndexMultimapTable {
        ParentRootIndexMultimapTable {
            db: self.db.clone(),
        }
    }

    pub fn proposer_boost_root_provider(&self) -> ProposerBoostRootField {
        ProposerBoostRootField {
            db: self.db.clone(),
        }
    }

    pub fn unrealized_finalized_checkpoint_provider(&self) -> UnrealizedFinalizedCheckpointField {
        UnrealizedFinalizedCheckpointField {
            db: self.db.clone(),
        }
    }

    pub fn unrealized_justified_checkpoint_provider(&self) -> UnrealizedJustifiedCheckpointField {
        UnrealizedJustifiedCheckpointField {
            db: self.db.clone(),
        }
    }

    pub fn finalized_checkpoint_provider(&self) -> FinalizedCheckpointField {
        FinalizedCheckpointField {
            db: self.db.clone(),
        }
    }

    pub fn justified_checkpoint_provider(&self) -> JustifiedCheckpointField {
        JustifiedCheckpointField {
            db: self.db.clone(),
        }
    }

    pub fn genesis_time_provider(&self) -> GenesisTimeField {
        GenesisTimeField {
            db: self.db.clone(),
        }
    }

    pub fn time_provider(&self) -> TimeField {
        TimeField {
            db: self.db.clone(),
        }
    }

    pub fn equivocating_indices_provider(&self) -> EquivocatingIndicesField {
        EquivocatingIndicesField {
            db: self.db.clone(),
        }
    }

    pub fn slot_index_provider(&self) -> SlotIndexTable {
        SlotIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn state_root_index_provider(&self) -> StateRootIndexTable {
        StateRootIndexTable {
            db: self.db.clone(),
        }
    }

    pub fn is_initialized(&self) -> bool {
        match self.slot_index_provider().get_highest_slot() {
            Ok(Some(slot)) => slot > 0,
            _ => false,
        }
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

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use redb::{Builder, Database};

use crate::{
    dir,
    errors::StoreError,
    tables::{
        beacon_block::BeaconBlockTable, beacon_state::BeaconStateTable,
        block_timeliness::BlockTimelinessTable, checkpoint_states::CheckpointStatesTable,
        finalized_checkpoint::FinalizedCheckpointField, genesis_time::GenesisTimeField,
        justified_checkpoint::JustifiedCheckpointField, latest_messages::LatestMessagesTable,
        proposer_boost_root::ProposerBoostRootField,
        unrealized_finalized_checkpoint::UnrealizedFinalizedCheckpointField,
        unrealized_justifications::UnrealizedJustificationsTable,
        unrealized_justified_checkpoint::UnrealizedJustifiedCheckpointField,
    },
};

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
}

use std::str::FromStr;

use ream_fork_choice::store::Store;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
pub mod checkpoint;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum SyncMode {
    CheckpointSync,
}

impl FromStr for SyncMode {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "checkpoint-sync" => Ok(SyncMode::CheckpointSync),
            _ => Err("Unknown sync mode"),
        }
    }
}

impl SyncMode {
    pub fn is_checkpoint_sync(&self) -> bool {
        match self {
            SyncMode::CheckpointSync => true,
        }
    }

    pub async fn sync(&self, db: ReamDB, rpc: &str) -> anyhow::Result<(Store, u64)> {
        match self {
            SyncMode::CheckpointSync => checkpoint::checkpoint_sync(db, rpc).await,
        }
    }
}

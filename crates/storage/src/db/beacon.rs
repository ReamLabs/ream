use std::{path::PathBuf, sync::Arc};

use anyhow::anyhow;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use redb::Database;

use crate::tables::{
    beacon::{
        beacon_block::BeaconBlockTable, beacon_state::BeaconStateTable,
        blobs_and_proofs::BlobsAndProofsTable, block_timeliness::BlockTimelinessTable,
        checkpoint_states::CheckpointStatesTable, equivocating_indices::EquivocatingIndicesField,
        finalized_checkpoint::FinalizedCheckpointField, genesis_time::GenesisTimeField,
        justified_checkpoint::JustifiedCheckpointField, latest_messages::LatestMessagesTable,
        parent_root_index::ParentRootIndexMultimapTable,
        proposer_boost_root::ProposerBoostRootField, slot_index::SlotIndexTable,
        state_root_index::StateRootIndexTable, time::TimeField,
        unrealized_finalized_checkpoint::UnrealizedFinalizedCheckpointField,
        unrealized_justifications::UnrealizedJustificationsTable,
        unrealized_justified_checkpoint::UnrealizedJustifiedCheckpointField,
    },
    table::Table,
};

#[derive(Clone, Debug)]
pub struct BeaconDB {
    pub db: Arc<Database>,
    pub data_dir: PathBuf,
}

impl BeaconDB {
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
            data_dir: self.data_dir.clone(),
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

    pub fn get_latest_state(&self) -> anyhow::Result<BeaconState> {
        let highest_root = self
            .slot_index_provider()
            .get_highest_root()?
            .expect("No highest root found");

        let state = self
            .beacon_state_provider()
            .get(highest_root)?
            .ok_or_else(|| anyhow!("Unable to fetch latest state"))?;

        Ok(state)
    }
}

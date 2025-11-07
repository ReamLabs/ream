use std::sync::Arc;

use redb::Database;

use crate::tables::lean::{
    latest_finalized::LatestFinalizedField, latest_justified::LatestJustifiedField,
    latest_known_attestation::LatestKnownAttestationTable, lean_block::LeanBlockTable,
    lean_head::LeanHeadField, lean_safe_target::LeanSafeTargetField, lean_state::LeanStateTable,
    lean_time::LeanTimeField, slot_index::SlotIndexTable, state_root_index::StateRootIndexTable,
};

#[derive(Clone, Debug)]
pub struct LeanDB {
    pub db: Arc<Database>,
}

impl LeanDB {
    pub fn lean_block_provider(&self) -> LeanBlockTable {
        LeanBlockTable {
            db: self.db.clone(),
        }
    }
    pub fn lean_state_provider(&self) -> LeanStateTable {
        LeanStateTable {
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

    pub fn latest_known_attestations_provider(&self) -> LatestKnownAttestationTable {
        LatestKnownAttestationTable {
            db: self.db.clone(),
        }
    }

    pub fn latest_finalized_provider(&self) -> LatestFinalizedField {
        LatestFinalizedField {
            db: self.db.clone(),
        }
    }

    pub fn latest_justified_provider(&self) -> LatestJustifiedField {
        LatestJustifiedField {
            db: self.db.clone(),
        }
    }

    pub fn lean_time_provider(&self) -> LeanTimeField {
        LeanTimeField {
            db: self.db.clone(),
        }
    }

    pub fn lean_head_provider(&self) -> LeanHeadField {
        LeanHeadField {
            db: self.db.clone(),
        }
    }

    pub fn lean_safe_target_provider(&self) -> LeanSafeTargetField {
        LeanSafeTargetField {
            db: self.db.clone(),
        }
    }
}

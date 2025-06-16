use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(Debug, Deserialize, Serialize, Encode, Decode, Default)]
pub struct SyncStatus {
    #[serde(with = "serde_utils::quoted_u64")]
    pub head_slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub sync_distance: u64,
    pub is_syncing: bool,
    pub is_optimistic: bool,
    pub el_offline: bool,
}

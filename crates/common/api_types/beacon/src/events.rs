use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub block: B256,
    pub state: B256,
    pub epoch_transition: bool,
    pub previous_duty_dependent_root: B256,
    pub current_duty_dependent_root: B256,
    pub execution_optimistic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedCheckpointEvent {
    pub block: B256,
    pub state: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub epoch: u64,
    pub execution_optimistic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainReorgEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub depth: u64,
    pub old_head_block: B256,
    pub new_head_block: B256,
    pub old_head_state: B256,
    pub new_head_state: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub epoch: u64,
    pub execution_optimistic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum BeaconEvent {
    Head(HeadEvent),
    Block(Box<SignedBeaconBlock>),
}

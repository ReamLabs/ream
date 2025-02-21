use alloy_primitives::U256;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyncingInfo {
    pub starting_block: U256,
    pub current_block: U256,
    pub highest_block: U256,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum EthSyncing {
    SyncingInfo(SyncingInfo),
    NotSyncing(bool),
}

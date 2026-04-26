use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTest {
    pub network: String,
    pub operation: String,
    pub input: SyncInput,
    pub output: SyncOutput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncInput {
    pub num_validators: u64,
    #[serde(default)]
    pub anchor_slot: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutput {
    pub valid: bool,
    pub state_bytes: String,
    pub validator_count: u64,
    pub anchor_slot: u64,
}

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTest {
    pub network: String,
    /// devnet4: plain string; devnet5: object with `kind`, `numValidators`, `anchorSlot`
    pub operation: serde_json::Value,
    /// devnet4 only — params moved into `operation` in devnet5
    #[serde(default)]
    pub input: Option<SyncInput>,
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
    /// devnet4 only — in devnet5 this lives in `operation`
    #[serde(default)]
    pub validator_count: Option<u64>,
    /// devnet4 only — in devnet5 this lives in `operation`
    #[serde(default)]
    pub anchor_slot: Option<u64>,
}

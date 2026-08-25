use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncTest {
    pub network: String,
    /// An object with `kind`, `numValidators`, and `anchorSlot`.
    pub operation: serde_json::Value,
    pub output: SyncOutput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutput {
    pub valid: bool,
    pub state_bytes: String,
}

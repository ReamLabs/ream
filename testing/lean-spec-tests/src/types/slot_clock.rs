use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockTest {
    pub network: String,
    /// An object with `kind` and input params.
    pub operation: serde_json::Value,
    pub output: SlotClockOutput,
    #[serde(default)]
    pub config: Option<SlotClockConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockOutput {
    #[serde(default)]
    pub slot: Option<u64>,
    #[serde(default)]
    pub interval: Option<u64>,
    #[serde(default)]
    pub total_intervals: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockConfig {
    pub seconds_per_slot: u64,
    pub intervals_per_slot: u64,
    pub milliseconds_per_interval: u64,
}

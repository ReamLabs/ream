use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockTest {
    pub network: String,
    pub operation: String,
    pub input: serde_json::Value,
    pub output: SlotClockOutput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockOutput {
    pub config: SlotClockConfig,
    #[serde(default)]
    pub slot: Option<u64>,
    #[serde(default)]
    pub interval: Option<u64>,
    #[serde(default)]
    pub total_intervals: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotClockConfig {
    pub seconds_per_slot: u64,
    pub intervals_per_slot: u64,
    pub milliseconds_per_interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentTimeInput {
    pub genesis_time: u64,
    pub current_time_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FromSlotInput {
    pub slot: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FromUnixTimeInput {
    pub unix_seconds: u64,
    pub genesis_time: u64,
}

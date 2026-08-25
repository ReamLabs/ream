use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JustifiabilityTest {
    pub network: String,
    pub slot: u64,
    pub finalized_slot: u64,
    pub output: JustifiabilityOutput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JustifiabilityOutput {
    pub delta: i64,
    pub is_justifiable: bool,
}

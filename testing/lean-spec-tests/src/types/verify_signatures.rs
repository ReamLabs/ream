use serde::Deserialize;

use crate::types::ssz::{SignedBlockJSON, StateJSON};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySignaturesTest {
    pub network: String,
    #[serde(default)]
    pub expect_exception: Option<String>,
    pub anchor_state: StateJSON,
    pub signed_block: SignedBlockJSON,
}

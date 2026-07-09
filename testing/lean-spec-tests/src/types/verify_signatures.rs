use serde::Deserialize;

use crate::types::ssz::{SignedBlockJSON, StateJSON};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifySignaturesTest {
    pub network: String,
    #[serde(default)]
    pub rejection_reason: Option<String>,
    pub anchor_state: StateJSON,
    pub signed_block: SignedBlockJSON,
}

impl VerifySignaturesTest {
    pub fn expected_rejection(&self) -> Option<&str> {
        self.rejection_reason.as_deref()
    }
}

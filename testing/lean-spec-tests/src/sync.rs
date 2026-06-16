use std::path::Path;

use alloy_primitives::hex;
use anyhow::{anyhow, bail, ensure};
use ream_consensus_lean::state::LeanState;
use ssz::Decode;
use tracing::info;

use crate::types::{TestFixture, sync::SyncTest};

/// Load a sync test fixture from a JSON file
pub fn load_sync_test(path: impl AsRef<Path>) -> anyhow::Result<TestFixture<SyncTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Run a single sync test case (currently the only operation is `verify_checkpoint`)
pub fn run_sync_test(test_name: &str, test: &SyncTest) -> anyhow::Result<()> {
    // devnet4: operation is a plain string, params in output
    // devnet5: operation is an object with kind + params
    let (operation_kind, num_validators, expected_anchor_slot) =
        if let Some(kind) = test.operation.as_str() {
            let num_validators = test
                .output
                .validator_count
                .ok_or_else(|| anyhow!("devnet4 format requires output.validatorCount"))?;
            let anchor_slot = test
                .output
                .anchor_slot
                .ok_or_else(|| anyhow!("devnet4 format requires output.anchorSlot"))?;
            (kind.to_string(), num_validators, anchor_slot)
        } else {
            let kind = test.operation["kind"]
                .as_str()
                .ok_or_else(|| anyhow!("Missing operation.kind"))?
                .to_string();
            let num_validators = test.operation["numValidators"]
                .as_u64()
                .ok_or_else(|| anyhow!("Missing operation.numValidators"))?;
            let anchor_slot = test.operation["anchorSlot"].as_u64().unwrap_or(0);
            (kind, num_validators, anchor_slot)
        };

    info!("Running sync test: {test_name} (operation={operation_kind})");

    if operation_kind != "verify_checkpoint" {
        bail!("Unknown sync operation: {operation_kind}");
    }

    let state_bytes = hex::decode(test.output.state_bytes.trim_start_matches("0x"))
        .map_err(|err| anyhow!("Failed to decode stateBytes hex: {err}"))?;

    let state = LeanState::from_ssz_bytes(&state_bytes)
        .map_err(|err| anyhow!("Failed to SSZ-decode anchor state: {err:?}"))?;

    let actual_validator_count = state.validators.len() as u64;
    let actual_slot = state.slot;
    let actually_valid = actual_validator_count > 0;

    ensure!(
        actual_validator_count == num_validators,
        "validatorCount mismatch: expected {num_validators}, got {actual_validator_count}",
    );
    ensure!(
        actual_slot == expected_anchor_slot,
        "anchorSlot mismatch: expected {expected_anchor_slot}, got {actual_slot}",
    );
    ensure!(
        actually_valid == test.output.valid,
        "valid mismatch: expected {}, got {actually_valid}",
        test.output.valid,
    );

    Ok(())
}

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
    info!(
        "Running sync test: {test_name} (operation={})",
        test.operation
    );

    if test.operation != "verify_checkpoint" {
        bail!("Unknown sync operation: {}", test.operation);
    }

    let state_bytes = hex::decode(test.output.state_bytes.trim_start_matches("0x"))
        .map_err(|err| anyhow!("Failed to decode stateBytes hex: {err}"))?;

    let state = LeanState::from_ssz_bytes(&state_bytes)
        .map_err(|err| anyhow!("Failed to SSZ-decode anchor state: {err:?}"))?;

    let actual_validator_count = state.validators.len() as u64;
    let actual_slot = state.slot;
    let actually_valid = actual_validator_count > 0;

    ensure!(
        actual_validator_count == test.output.validator_count,
        "validatorCount mismatch: expected {}, got {actual_validator_count}",
        test.output.validator_count,
    );
    ensure!(
        actual_slot == test.output.anchor_slot,
        "anchorSlot mismatch: expected {}, got {actual_slot}",
        test.output.anchor_slot,
    );
    ensure!(
        actually_valid == test.output.valid,
        "valid mismatch: expected {}, got {actually_valid}",
        test.output.valid,
    );

    Ok(())
}

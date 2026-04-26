use std::path::Path;

use anyhow::{anyhow, ensure};
use ream_consensus_lean::slot::is_justifiable_after;
use tracing::info;

use crate::types::{TestFixture, justifiability::JustifiabilityTest};

/// Load a justifiability test fixture from a JSON file
pub fn load_justifiability_test(
    path: impl AsRef<Path>,
) -> anyhow::Result<TestFixture<JustifiabilityTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Run a single justifiability test case
pub fn run_justifiability_test(test_name: &str, test: &JustifiabilityTest) -> anyhow::Result<()> {
    info!("Running justifiability test: {test_name}");
    info!(
        "  slot={} finalized_slot={} expected delta={} is_justifiable={}",
        test.slot, test.finalized_slot, test.output.delta, test.output.is_justifiable,
    );

    ensure!(
        test.slot >= test.finalized_slot,
        "Fixture has slot ({}) < finalized_slot ({})",
        test.slot,
        test.finalized_slot,
    );

    let actual_delta = test.slot - test.finalized_slot;
    ensure!(
        actual_delta == test.output.delta,
        "Delta mismatch: expected {}, got {actual_delta}",
        test.output.delta,
    );

    let actual = is_justifiable_after(test.slot, test.finalized_slot)?;
    ensure!(
        actual == test.output.is_justifiable,
        "is_justifiable mismatch: expected {}, got {actual}",
        test.output.is_justifiable,
    );

    Ok(())
}

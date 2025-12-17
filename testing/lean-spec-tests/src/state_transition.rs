use std::path::Path;

use anyhow::{anyhow, bail, ensure};
use ream_consensus_lean::{block::Block as ReamBlock, state::LeanState};
use tracing::{debug, info};

use crate::types::{TestFixture, state_transition::StateTransitionTest};

/// Load a state transition test fixture from a JSON file
pub fn load_state_transition_test(
    path: impl AsRef<Path>,
) -> anyhow::Result<TestFixture<StateTransitionTest>> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|err| {
        anyhow!(
            "Failed to read test file {:?}: {err}",
            path.as_ref().display()
        )
    })?;

    let fixture: TestFixture<StateTransitionTest> =
        serde_json::from_str(&content).map_err(|err| {
            anyhow!(
                "Failed to parse test file {:?}: {err}",
                path.as_ref().display()
            )
        })?;

    Ok(fixture)
}

/// Run a single state transition test case
pub fn run_state_transition_test(
    test_name: &str,
    test: &StateTransitionTest,
) -> anyhow::Result<()> {
    info!("Running state transition test: {test_name}");
    info!("  Network: {}", test.network);
    info!("  Pre-state slot: {}", test.pre.slot);
    info!("  Number of blocks: {}", test.blocks.len());

    // Convert pre-state to LeanState
    let mut state = LeanState::try_from(test.pre.clone())
        .map_err(|err| anyhow!("Failed to convert pre-state: {err}"))?;

    // Track whether we expect an exception
    let expect_exception = test.expect_exception.is_some();
    if expect_exception {
        info!(
            "Expected result: Exception: {}",
            test.expect_exception
                .as_ref()
                .expect("Failed to fetch expected exception")
        );
    } else {
        info!("Expected result: Success");
    }

    // Process each block
    let mut result = Ok(());
    for (index, block) in test.blocks.iter().enumerate() {
        debug!("Processing block {} at slot {}", index, block.slot);

        // Convert test block to ream block
        let ream_block = match ReamBlock::try_from(block) {
            Ok(block) => block,
            Err(err) => {
                result = Err(anyhow!("Failed to convert block {index}: {err}"));
                break;
            }
        };

        match state.state_transition(&ream_block, true) {
            Ok(_) => {
                debug!("    Block {} processed successfully", index);
            }
            Err(err) => {
                result = Err(anyhow!("State transition failed for block {index}: {err}"));
                break;
            }
        }
    }

    // Check if the result matches expectations
    match (result, expect_exception) {
        (Ok(_), true) => {
            bail!(
                "Expected exception '{}' but state transition succeeded",
                test.expect_exception
                    .as_ref()
                    .expect("Failed to fetch expected exception")
            );
        }
        (Err(err), false) => {
            bail!("State transition should succeed but failed: {err}");
        }
        (Err(err), true) => {
            info!("Got expected exception: {err}");
        }
        (Ok(_), false) => {
            info!("State transition succeeded as expected");

            // Validate post-state expectations if provided
            if let Some(post) = &test.post {
                validate_post_state(&state, post)?;
            }
        }
    }

    info!("Test passed");
    Ok(())
}

/// Validate the post-state against expectations
fn validate_post_state(
    state: &LeanState,
    expectations: &crate::types::state_transition::StateExpectation,
) -> anyhow::Result<()> {
    info!("  Validating post-state expectations:");

    if let Some(expected_slot) = expectations.slot {
        ensure!(
            state.slot == expected_slot,
            "Post-state slot mismatch: expected {expected_slot}, got {}",
            state.slot
        );
        info!("slot: {}", state.slot);
    }

    if let Some(expected_header_slot) = expectations.latest_block_header_slot {
        ensure!(
            state.latest_block_header.slot == expected_header_slot,
            "Post-state latest_block_header.slot mismatch: expected {expected_header_slot}, got {}",
            state.latest_block_header.slot
        );
        info!(
            "latest_block_header.slot: {}",
            state.latest_block_header.slot
        );
    }

    if let Some(expected_state_root) = expectations.latest_block_header_state_root {
        ensure!(
            state.latest_block_header.state_root == expected_state_root,
            "Post-state latest_block_header.state_root mismatch: expected {expected_state_root}, got {}",
            state.latest_block_header.state_root
        );
        info!(
            "latest_block_header.state_root: {}",
            state.latest_block_header.state_root
        );
    }

    if let Some(expected_count) = expectations.historical_block_hashes_count {
        let actual_count = state.historical_block_hashes.len();
        ensure!(
            actual_count == expected_count,
            "Post-state historical_block_hashes count mismatch: expected {expected_count}, got {actual_count}"
        );
        info!("historical_block_hashes.len(): {actual_count}");
    }

    Ok(())
}

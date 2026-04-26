use std::path::Path;

use anyhow::{anyhow, bail};
use ream_consensus_lean::{block::SignedBlock, state::LeanState};
use tracing::info;

use crate::types::{TestFixture, verify_signatures::VerifySignaturesTest};

/// Load a verify_signatures test fixture from a JSON file
pub fn load_verify_signatures_test(
    path: impl AsRef<Path>,
) -> anyhow::Result<TestFixture<VerifySignaturesTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Run a single verify_signatures test case
pub fn run_verify_signatures_test(
    test_name: &str,
    test: &VerifySignaturesTest,
) -> anyhow::Result<()> {
    info!("Running verify_signatures test: {test_name}");

    let parent_state = LeanState::try_from(&test.anchor_state)
        .map_err(|err| anyhow!("Failed to convert anchor state: {err}"))?;

    let signed_block = match SignedBlock::try_from(&test.signed_block) {
        Ok(block) => block,
        Err(err) => {
            // A conversion failure (e.g. malformed signature length) is itself
            // a structural rejection. If the fixture expects an exception,
            // count this as the expected outcome.
            if test.expect_exception.is_some() {
                info!("Got expected conversion error: {err}");
                return Ok(());
            }
            return Err(anyhow!("Failed to convert signed block: {err}"));
        }
    };

    let result = signed_block.verify_signatures(&parent_state, true);

    match (result, test.expect_exception.as_ref()) {
        (Ok(_), Some(exception)) => {
            bail!("Expected exception '{exception}' but verify_signatures succeeded");
        }
        (Err(err), None) => {
            bail!("verify_signatures should succeed but failed: {err}");
        }
        (Err(err), Some(_)) => {
            info!("Got expected exception: {err}");
        }
        (Ok(_), None) => {
            info!("verify_signatures succeeded as expected");
        }
    }
    Ok(())
}

use std::path::Path;

use anyhow::{Result, anyhow};

use crate::types::{TestFixture, state_transition::StateTransitionTest};

/// Load a state transition test fixture from a JSON file
pub fn load_state_transition_test(
    path: impl AsRef<Path>,
) -> Result<TestFixture<StateTransitionTest>> {
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

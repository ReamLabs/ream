use std::path::Path;

use anyhow::{anyhow, bail, ensure};
use tracing::info;

use crate::types::{
    TestFixture,
    slot_clock::{CurrentTimeInput, FromSlotInput, FromUnixTimeInput, SlotClockTest},
};

/// Load a slot_clock test fixture from a JSON file
pub fn load_slot_clock_test(path: impl AsRef<Path>) -> anyhow::Result<TestFixture<SlotClockTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Run a single slot_clock test case
pub fn run_slot_clock_test(test_name: &str, test: &SlotClockTest) -> anyhow::Result<()> {
    info!(
        "Running slot_clock test: {test_name} (operation={})",
        test.operation
    );

    let cfg = &test.output.config;
    ensure!(
        cfg.seconds_per_slot * 1000 == cfg.intervals_per_slot * cfg.milliseconds_per_interval,
        "Inconsistent config: secondsPerSlot * 1000 != intervalsPerSlot * msPerInterval",
    );

    let ms_per_slot = cfg.seconds_per_slot * 1000;
    let ms_per_interval = cfg.milliseconds_per_interval;

    match test.operation.as_str() {
        "current_slot" => {
            let input: CurrentTimeInput = serde_json::from_value(test.input.clone())?;
            let expected = test
                .output
                .slot
                .ok_or_else(|| anyhow!("Missing output.slot"))?;
            let genesis_ms = input.genesis_time * 1000;
            let actual = if input.current_time_ms <= genesis_ms {
                0
            } else {
                (input.current_time_ms - genesis_ms) / ms_per_slot
            };
            ensure!(
                actual == expected,
                "current_slot mismatch: expected {expected}, got {actual}"
            );
        }
        "current_interval" => {
            let input: CurrentTimeInput = serde_json::from_value(test.input.clone())?;
            let expected = test
                .output
                .interval
                .ok_or_else(|| anyhow!("Missing output.interval"))?;
            let genesis_ms = input.genesis_time * 1000;
            let actual = if input.current_time_ms <= genesis_ms {
                0
            } else {
                let elapsed = input.current_time_ms - genesis_ms;
                (elapsed % ms_per_slot) / ms_per_interval
            };
            ensure!(
                actual == expected,
                "current_interval mismatch: expected {expected}, got {actual}"
            );
        }
        "total_intervals" => {
            let input: CurrentTimeInput = serde_json::from_value(test.input.clone())?;
            let expected = test
                .output
                .total_intervals
                .ok_or_else(|| anyhow!("Missing output.totalIntervals"))?;
            let genesis_ms = input.genesis_time * 1000;
            let actual = if input.current_time_ms <= genesis_ms {
                0
            } else {
                (input.current_time_ms - genesis_ms) / ms_per_interval
            };
            ensure!(
                actual == expected,
                "total_intervals mismatch: expected {expected}, got {actual}"
            );
        }
        "from_slot" => {
            let input: FromSlotInput = serde_json::from_value(test.input.clone())?;
            let expected = test
                .output
                .interval
                .ok_or_else(|| anyhow!("Missing output.interval"))?;
            let actual = input.slot * cfg.intervals_per_slot;
            ensure!(
                actual == expected,
                "from_slot mismatch: expected {expected}, got {actual}"
            );
        }
        "from_unix_time" => {
            let input: FromUnixTimeInput = serde_json::from_value(test.input.clone())?;
            let expected = test
                .output
                .interval
                .ok_or_else(|| anyhow!("Missing output.interval"))?;
            let actual = if input.unix_seconds <= input.genesis_time {
                0
            } else {
                ((input.unix_seconds - input.genesis_time) * 1000) / ms_per_interval
            };
            ensure!(
                actual == expected,
                "from_unix_time mismatch: expected {expected}, got {actual}"
            );
        }
        other => bail!("Unknown slot_clock operation: {other}"),
    }
    Ok(())
}

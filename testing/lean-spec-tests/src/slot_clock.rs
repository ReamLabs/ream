use std::path::Path;

use anyhow::{anyhow, bail, ensure};
use tracing::info;

use crate::types::{TestFixture, slot_clock::SlotClockTest};

/// Load a slot_clock test fixture from a JSON file
pub fn load_slot_clock_test(path: impl AsRef<Path>) -> anyhow::Result<TestFixture<SlotClockTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Parse a JSON value as u64, accepting both integer and float representations.
fn json_u64(v: &serde_json::Value, field: &str) -> anyhow::Result<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().map(|f| f as u64))
        .ok_or_else(|| anyhow!("Missing or non-numeric field: {field}"))
}

/// Run a single slot_clock test case
pub fn run_slot_clock_test(test_name: &str, test: &SlotClockTest) -> anyhow::Result<()> {
    // Config lives at top-level in devnet5, inside output in devnet4.
    let cfg = test
        .config
        .as_ref()
        .or(test.output.config.as_ref())
        .ok_or_else(|| anyhow!("Missing config"))?;

    // Operation kind: plain string in devnet4, object with `kind` field in devnet5.
    let kind: &str = if let Some(s) = test.operation.as_str() {
        s
    } else {
        test.operation["kind"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing operation.kind"))?
    };

    info!("Running slot_clock test: {test_name} (operation={kind})");

    ensure!(
        cfg.seconds_per_slot * 1000 == cfg.intervals_per_slot * cfg.milliseconds_per_interval,
        "Inconsistent config: secondsPerSlot * 1000 != intervalsPerSlot * msPerInterval",
    );

    let ms_per_slot = cfg.seconds_per_slot * 1000;
    let ms_per_interval = cfg.milliseconds_per_interval;

    // In devnet4 params are in `input`; in devnet5 they are in the `operation` object.
    let params: &serde_json::Value = if let Some(inp) = test.input.as_ref() {
        inp
    } else {
        &test.operation
    };

    // Look up a u64 param, falling back to an alternate key name if the first is missing.
    let param_u64 = |key: &str, alt: Option<&str>| -> anyhow::Result<u64> {
        let v = params
            .get(key)
            .or_else(|| alt.and_then(|k| params.get(k)))
            .ok_or_else(|| anyhow!("Missing param: {key}"))?;
        json_u64(v, key)
    };

    match kind {
        "current_slot" => {
            let genesis_time = param_u64("genesisTime", None)?;
            // devnet4: currentTimeMs (integer), devnet5: currentTimeMilliseconds (float)
            let current_time_ms = param_u64("currentTimeMs", Some("currentTimeMilliseconds"))?;
            let expected = test.output.slot.ok_or_else(|| anyhow!("Missing output.slot"))?;
            let genesis_ms = genesis_time * 1000;
            let actual = if current_time_ms <= genesis_ms {
                0
            } else {
                (current_time_ms - genesis_ms) / ms_per_slot
            };
            ensure!(actual == expected, "current_slot mismatch: expected {expected}, got {actual}");
        }
        "current_interval" => {
            let genesis_time = param_u64("genesisTime", None)?;
            let current_time_ms = param_u64("currentTimeMs", Some("currentTimeMilliseconds"))?;
            let expected =
                test.output.interval.ok_or_else(|| anyhow!("Missing output.interval"))?;
            let genesis_ms = genesis_time * 1000;
            let actual = if current_time_ms <= genesis_ms {
                0
            } else {
                let elapsed = current_time_ms - genesis_ms;
                (elapsed % ms_per_slot) / ms_per_interval
            };
            ensure!(
                actual == expected,
                "current_interval mismatch: expected {expected}, got {actual}"
            );
        }
        "total_intervals" => {
            let genesis_time = param_u64("genesisTime", None)?;
            let current_time_ms = param_u64("currentTimeMs", Some("currentTimeMilliseconds"))?;
            let expected = test
                .output
                .total_intervals
                .ok_or_else(|| anyhow!("Missing output.totalIntervals"))?;
            let genesis_ms = genesis_time * 1000;
            let actual = if current_time_ms <= genesis_ms {
                0
            } else {
                (current_time_ms - genesis_ms) / ms_per_interval
            };
            ensure!(
                actual == expected,
                "total_intervals mismatch: expected {expected}, got {actual}"
            );
        }
        "from_slot" => {
            let slot = param_u64("slot", None)?;
            let expected =
                test.output.interval.ok_or_else(|| anyhow!("Missing output.interval"))?;
            let actual = slot * cfg.intervals_per_slot;
            ensure!(actual == expected, "from_slot mismatch: expected {expected}, got {actual}");
        }
        "from_unix_time" => {
            let genesis_time = param_u64("genesisTime", None)?;
            let unix_seconds = param_u64("unixSeconds", None)?;
            let expected =
                test.output.interval.ok_or_else(|| anyhow!("Missing output.interval"))?;
            let actual = if unix_seconds <= genesis_time {
                0
            } else {
                ((unix_seconds - genesis_time) * 1000) / ms_per_interval
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

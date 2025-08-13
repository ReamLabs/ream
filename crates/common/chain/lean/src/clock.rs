use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_network_spec::networks::lean_network_spec;
use tokio::time::{Instant, Interval, MissedTickBehavior, interval_at};

pub fn create_lean_clock_interval() -> anyhow::Result<Interval> {
    let genesis_instant = UNIX_EPOCH + Duration::from_secs(lean_network_spec().genesis_time);

    let interval_start = Instant::now()
        + genesis_instant
            .duration_since(SystemTime::now())
            .map_err(|err| anyhow!("Genesis time is in the past: {err}"))?;

    let mut interval = interval_at(
        interval_start,
        Duration::from_secs(lean_network_spec().seconds_per_slot / INTERVALS_PER_SLOT),
    );
    interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

    Ok(interval)
}

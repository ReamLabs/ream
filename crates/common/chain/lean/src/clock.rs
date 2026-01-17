use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_network_spec::networks::lean_network_spec;
use tokio::time::{Instant, Interval, MissedTickBehavior, interval_at};

pub fn create_lean_clock_interval() -> anyhow::Result<Interval> {
    let now = SystemTime::now();
    let genesis_instant = UNIX_EPOCH + Duration::from_secs(lean_network_spec().genesis_time);

    let tick_duration =
        Duration::from_secs(lean_network_spec().seconds_per_slot) / INTERVALS_PER_SLOT as u32;

    let interval_start = if now < genesis_instant {
        Instant::now()
            + genesis_instant
                .duration_since(now)
                .map_err(|err| anyhow!("System time seems to have drifted backwards: {err:?}"))?
    } else {
        let tick_micros = tick_duration.as_micros();
        let elapsed_micros = now
            .duration_since(genesis_instant)
            .map_err(|err| anyhow!("Failed to calculate elapsed time since genesis: {err:?}"))?
            .as_micros();

        let time_until_next_tick = tick_micros - (elapsed_micros % tick_micros);
        Instant::now() + Duration::from_micros(time_until_next_tick as u64)
    };

    let mut interval = interval_at(interval_start, tick_duration);
    interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

    Ok(interval)
}

pub fn get_initial_tick_count() -> u64 {
    let genesis_instant = UNIX_EPOCH + Duration::from_secs(lean_network_spec().genesis_time);
    let now = SystemTime::now();
    if now < genesis_instant {
        0
    } else {
        let elapsed = now
            .duration_since(genesis_instant)
            .unwrap_or(Duration::ZERO);
        let tick_duration =
            Duration::from_secs(lean_network_spec().seconds_per_slot) / INTERVALS_PER_SLOT as u32;

        (elapsed.as_millis() / tick_duration.as_millis()) as u64 + 1
    }
}

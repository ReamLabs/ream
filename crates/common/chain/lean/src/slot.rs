use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ream_network_spec::networks::lean_network_spec;
use tracing::warn;

pub fn get_current_slot() -> u64 {
    let spec = lean_network_spec();
    let seconds_per_slot = spec.seconds_per_slot;
    let genesis_time = spec.genesis_time;

    let genesis_instant = UNIX_EPOCH + Duration::from_secs(genesis_time);

    let elapsed = match SystemTime::now().duration_since(genesis_instant) {
        Ok(duration) => duration.as_secs(),
        // If before genesis, return 0
        Err(err) => {
            warn!("System time is before genesis time: {err}");
            0
        }
    };

    elapsed / seconds_per_slot
}

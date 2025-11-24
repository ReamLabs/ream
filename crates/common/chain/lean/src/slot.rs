use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ream_network_spec::networks::lean_network_spec;

/// NOTE: Vitalik's implementation of 3SF-mini adds 2 slots more due to the test setup.
/// This is due to the fact that his test code starts at slot 1.
pub fn get_current_slot() -> u64 {
    let seconds_per_slot = lean_network_spec().seconds_per_slot;
    let genesis_time = lean_network_spec().genesis_time;

    let genesis_instant = UNIX_EPOCH + Duration::from_secs(genesis_time);
    let elapsed = SystemTime::now()
        .duration_since(genesis_instant)
        .expect("Called before genesis time");

    elapsed.as_secs() / seconds_per_slot
}

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use ream_consensus::misc::compute_epoch_at_slot;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::NetworkSpec;
use tokio::{
    task::JoinHandle,
    time::{Instant, MissedTickBehavior, interval_at},
};

pub fn start_clock<SlotFunction, EpochFunction>(
    executor: &ReamExecutor,
    spec: &NetworkSpec,
    genesis_time: u64,
    mut slot_callback: SlotFunction,
    mut epoch_callback: EpochFunction,
) -> anyhow::Result<JoinHandle<()>>
where
    SlotFunction: FnMut(u64) + Send + 'static,
    EpochFunction: FnMut(u64) + Send + 'static,
{
    let seconds_per_slot = spec.seconds_per_slot;
    let genesis_instant = UNIX_EPOCH + Duration::from_secs(genesis_time);
    let elapsed = SystemTime::now()
        .duration_since(genesis_instant)
        .map_err(|err| anyhow!(format!("System Time is before the genesis time: {err:?}")))?;

    let mut slot = elapsed.as_secs() / seconds_per_slot;
    let mut epoch = compute_epoch_at_slot(slot);

    let mut interval = {
        let interval_start =
            Instant::now() - (elapsed - Duration::from_secs(slot * seconds_per_slot));
        interval_at(interval_start, Duration::from_secs(seconds_per_slot))
    };
    interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

    Ok(executor.spawn(async move {
        epoch_callback(epoch);
        slot_callback(slot);
        loop {
            interval.tick().await;

            slot += 1;
            let current_epoch = compute_epoch_at_slot(slot);

            if current_epoch != epoch {
                epoch = current_epoch;
                epoch_callback(epoch);
            }
            slot_callback(slot);
        }
    }))
}

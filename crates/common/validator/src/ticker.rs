use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Error, Result, anyhow};
use ream_consensus::misc::compute_epoch_at_slot;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::NetworkSpec;
use tokio::task::JoinHandle;

pub struct Ticker {
    pub slot: Arc<AtomicU64>,
    pub epoch: Arc<AtomicU64>,
    pub handle: JoinHandle<Result<(), Error>>,
}

impl Ticker {
    pub fn new<F>(
        executor: &ReamExecutor,
        spec: &NetworkSpec,
        genesis_time: u64,
        mut slot_callback: F,
        mut epoch_callback: F,
    ) -> Result<Self>
    where
        F: FnMut(u64) + Send + 'static,
    {
        let seconds_per_slot = spec.seconds_per_slot;
        let genesis_instant = UNIX_EPOCH + Duration::from_secs(genesis_time);
        let elapsed = SystemTime::now()
            .duration_since(genesis_instant)
            .map_err(|err| anyhow!(format!("System Time is before the genesis time: {err:?}")))?;

        let slot_val = elapsed.as_secs() / seconds_per_slot;

        let slot = Arc::new(AtomicU64::new(slot_val));
        let epoch = Arc::new(AtomicU64::new(compute_epoch_at_slot(slot_val)));

        let slot_arc = Arc::clone(&slot);
        let epoch_arc = Arc::clone(&epoch);

        let handle = executor.spawn_blocking(move || -> Result<()> {
            epoch_callback(epoch_arc.load(Ordering::Relaxed));
            slot_callback(slot_arc.load(Ordering::Relaxed));
            loop {
                let next_slot_start = genesis_instant
                    + Duration::from_secs(
                        (slot_arc.load(Ordering::Relaxed) + 1) * seconds_per_slot,
                    );
                sleep(
                    next_slot_start
                        .duration_since(SystemTime::now())
                        .map_err(|err| {
                            anyhow!(format!("System Time is before the genesis time: {err:?}"))
                        })?,
                );

                let current_slot = slot_arc.fetch_add(1, Ordering::Relaxed);
                let current_epoch = compute_epoch_at_slot(current_slot);

                if current_epoch != epoch_arc.load(Ordering::Relaxed) {
                    epoch_arc.swap(current_epoch, Ordering::Relaxed);
                    epoch_callback(current_epoch);
                }
                slot_callback(current_slot);
            }
        });
        Ok(Self {
            slot,
            epoch,
            handle,
        })
    }
}

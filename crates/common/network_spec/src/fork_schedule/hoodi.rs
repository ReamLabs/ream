use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const HOODI_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    (fixed_bytes!("0x10000910"), 0),     // Phase0
    (fixed_bytes!("0x20000910"), 0),     // Altair
    (fixed_bytes!("0x30000910"), 0),     // Bellatrix
    (fixed_bytes!("0x40000910"), 0),     // Capella
    (fixed_bytes!("0x50000910"), 0),     // Deneb
    (fixed_bytes!("0x60000910"), 2_048), // Electra
));

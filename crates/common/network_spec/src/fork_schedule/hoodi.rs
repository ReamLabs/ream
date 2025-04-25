use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const HOODI_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x10000910", 0),     // Phase0
    ("0x20000910", 0),     // Altair
    ("0x30000910", 0),     // Bellatrix
    ("0x40000910", 0),     // Capella
    ("0x50000910", 0),     // Deneb
    ("0x60000910", 2_048), // Electra
));

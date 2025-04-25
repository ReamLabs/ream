use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const HOLESKY_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x01017000", 0),       // Phase0
    ("0x02017000", 0),       // Altair
    ("0x03017000", 0),       // Bellatrix
    ("0x04017000", 256),     // Capella
    ("0x05017000", 29_696),  // Deneb
    ("0x06017000", 115_968), // Electra
));

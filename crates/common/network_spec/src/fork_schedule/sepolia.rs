use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const SEPOLIA_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    (fixed_bytes!("0x90000069"), 0),       // Phase0
    (fixed_bytes!("0x90000070"), 50),      // Altair
    (fixed_bytes!("0x90000071"), 100),     // Bellatrix
    (fixed_bytes!("0x90000072"), 56_832),  // Capella
    (fixed_bytes!("0x90000073"), 132_608), // Deneb
    (fixed_bytes!("0x90000074"), 222_464), // Electra
));

use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const MAINNET_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    (fixed_bytes!("0x00000000"), 0),                       // Phase0
    (fixed_bytes!("0x01000000"), 74_240),                  // Altair
    (fixed_bytes!("0x02000000"), 144_896),                 // Bellatrix
    (fixed_bytes!("0x03000000"), 194_048),                 // Capella
    (fixed_bytes!("0x04000000"), 269_568),                 // Deneb
    (fixed_bytes!("0x05000000"), Fork::UNSCHEDULED_EPOCH), // Electra
));

use alloy_primitives::fixed_bytes;
use ream_consensus::fork::Fork;

use super::ForkSchedule;

pub const MAINNET_FORK_SCHEDULE: ForkSchedule = ForkSchedule::new(fork_array!(
    ("0x00000000", 0),                       // Phase0
    ("0x01000000", 74_240),                  // Altair
    ("0x02000000", 144_896),                 // Bellatrix
    ("0x03000000", 194_048),                 // Capella
    ("0x04000000", 269_568),                 // Deneb
    ("0x05000000", Fork::UNSCHEDULED_EPOCH), // Electra
));

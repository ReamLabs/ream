use alloy_primitives::{aliases::B32, fixed_bytes};

pub const ATTESTATION_SUBNET_COUNT: u64 = 64;
pub const DOMAIN_SELECTION_PROOF: B32 = fixed_bytes!("0x05000000");
pub const SYNC_COMMITTEE_SUBNET_COUNT: u64 = 4;
pub const TARGET_AGGREGATORS_PER_COMMITTEE: u64 = 16;

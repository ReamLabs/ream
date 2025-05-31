use std::time::Duration;

use alloy_primitives::{aliases::B32, fixed_bytes};

/// The maximum allowed size of uncompressed payload in gossipsub messages and RPC chunks
pub const MAX_PAYLOAD_SIZE: u64 = 10485760;
pub const MESSAGE_DOMAIN_VALID_SNAPPY: B32 = fixed_bytes!("0x01000000");
pub const MESSAGE_DOMAIN_INVALID_SNAPPY: B32 = fixed_bytes!("0x00000000");

pub const PING_INTERVAL_DURATION: Duration = Duration::from_secs(300);
pub const TARGET_PEER_COUNT: usize = 50;

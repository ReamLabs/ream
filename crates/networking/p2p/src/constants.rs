use alloy_primitives::{aliases::B32, fixed_bytes};

/// The maximum allowed size of uncompressed payload in gossipsub messages and RPC chunks
pub const MAX_PAYLOAD_SIZE: u64 = 10485760;
pub const MESSAGE_DOMAIN_VALID_SNAPPY: B32 = fixed_bytes!("0x01000000");
pub const MESSAGE_DOMAIN_INVALID_SNAPPY: B32 = fixed_bytes!("0x00000000");

pub const STATE_CONNECTED: &str = "connected";
pub const STATE_CONNECTING: &str = "connecting";
pub const STATE_DISCONNECTED: &str = "disconnected";
pub const STATE_DISCONNECTING: &str = "disconnecting";
pub const DIR_INBOUND: &str = "inbound";
pub const DIR_OUTBOUND: &str = "outbound";

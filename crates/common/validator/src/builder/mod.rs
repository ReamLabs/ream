pub mod bid;
pub mod blobs;
pub mod builder_bid;
pub mod builder_client;
pub mod validator_registration;
pub mod verify;

use alloy_primitives::{aliases::B32, fixed_bytes};

pub const DOMAIN_APPLICATION_BUILDER: B32 = fixed_bytes!("0x00000001");

use alloy_primitives::{aliases::B32, fixed_bytes};
use url::Url;

pub mod bid;
pub mod blobs;
pub mod builder_bid;
pub mod builder_client;
pub mod registration;
pub mod validator_registration;
pub mod verify;

pub const DOMAIN_APPLICATION_BUILDER: B32 = fixed_bytes!("0x00000001");
pub const MAX_REGISTRATION_LOOKAHEAD: u64 = 10;

#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub builder_enabled: bool,
    pub mev_relay_url: Url,
}

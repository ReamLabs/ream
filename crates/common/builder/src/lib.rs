use url::Url;

pub mod bid;
pub mod blinded_beacon_block;
pub mod blobs;
pub mod builder_api;
pub mod builder_bid;
pub mod registration;
pub mod validator_registration;
pub mod verify;

#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub builder_enabled: bool,
    pub mev_relay_url: Url,
}

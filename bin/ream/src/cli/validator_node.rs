use std::{net::IpAddr, time::Duration};

use clap::Parser;
use url::Url;

use crate::cli::constants::{
    DEFAULT_BEACON_API_ENDPOINT, DEFAULT_HTTP_ADDRESS, DEFAULT_KEY_MANAGER_HTTP_PORT,
    DEFAULT_REQUEST_TIMEOUT,
};

#[derive(Debug, Parser)]
pub struct ValidatorNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(long, help = "Set HTTP url of the beacon api endpoint", default_value = DEFAULT_BEACON_API_ENDPOINT)]
    pub beacon_api_endpoint: Url,

    #[arg(long, help = "Set HTTP request timeout for beacon api calls", default_value = DEFAULT_REQUEST_TIMEOUT, value_parser = duration_parser)]
    pub request_timeout: Duration,

    #[arg(long, help = "Set HTTP address of the key manager server", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub key_manager_http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port of the key manager server", default_value_t = DEFAULT_KEY_MANAGER_HTTP_PORT)]
    pub key_manager_http_port: u16,
}

pub fn duration_parser(duration_string: &str) -> Result<Duration, String> {
    Ok(Duration::from_secs(duration_string.parse().map_err(
        |err| format!("Could not parse the request timeout: {err:?}"),
    )?))
}

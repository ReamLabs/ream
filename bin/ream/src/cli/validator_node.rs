use std::{net::IpAddr, time::Duration};

use clap::Parser;
use ream_validator::beacon_api_client::http_client::ContentType;
use url::Url;

use super::constants::DEFAULT_CONTENT_TYPE;
use crate::cli::constants::{
    DEFAULT_BEACON_API_ENDPOINT, DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_PORT, DEFAULT_REQUEST_TIMEOUT,
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

    #[arg(long, help = "Set content type for beacon api calls", default_value = DEFAULT_CONTENT_TYPE, value_parser = content_type_parser)]
    pub beacon_api_content_type: ContentType,

    #[arg(long, help = "Set HTTP address of the key manager server", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub key_manager_http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port of the key manager server", default_value_t = DEFAULT_HTTP_PORT)]
    pub key_manager_http_port: u16,
}

pub fn duration_parser(duration_string: &str) -> Result<Duration, String> {
    Ok(Duration::from_secs(duration_string.parse().map_err(
        |err| format!("Could not parse the request timeout: {err:?}"),
    )?))
}

pub fn content_type_parser(content_type_string: &str) -> Result<ContentType, String> {
    match content_type_string {
        "ssz" => Ok(ContentType::Ssz),
        "json" => Ok(ContentType::Json),
        _ => Err(format!(
            "Invalid Content Type provided: {}",
            content_type_string
        )),
    }
}

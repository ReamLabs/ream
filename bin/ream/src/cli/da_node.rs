use std::net::IpAddr;

use clap::Parser;

use crate::cli::constants::{
    DEFAULT_DA_HTTP_PORT, DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN,
};

#[derive(Debug, Parser)]
pub struct DaNodeConfig {
    #[arg(long, default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,
    #[arg(long, default_value_t = DEFAULT_DA_HTTP_PORT)]
    pub http_port: u16,
    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,
}

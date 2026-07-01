use std::{net::IpAddr, sync::Arc};

use clap::Parser;
use ream_network_spec::{cli::beacon_network_parser, networks::BeaconNetworkSpec};

use crate::cli::constants::{
    DEFAULT_DA_HTTP_PORT, DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_NETWORK,
};

#[derive(Debug, Parser)]
pub struct DaNodeConfig {
    #[arg(
        long,
        help = "Choose mainnet, sepolia, hoodi, dev or provide a path to a YAML config file",
        default_value = DEFAULT_NETWORK,
        value_parser = beacon_network_parser
    )]
    pub network: Arc<BeaconNetworkSpec>,

    #[arg(long, default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,
    #[arg(long, default_value_t = DEFAULT_DA_HTTP_PORT)]
    pub http_port: u16,
    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,
}

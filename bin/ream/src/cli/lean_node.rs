use std::{net::IpAddr, sync::Arc};

use clap::Parser;
use ream_network_spec::{cli::lean_network_parser, networks::LeanNetworkSpec};

use crate::cli::constants::{DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_HTTP_PORT};

#[derive(Debug, Parser)]
pub struct LeanNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
      long,
      help = "Provide a path to a YAML config file",
      value_parser = lean_network_parser
  )]
    pub network: Arc<LeanNetworkSpec>,

    #[arg(long, help = "Set HTTP address", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port", default_value_t = DEFAULT_HTTP_PORT)]
    pub http_port: u16,

    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,
}

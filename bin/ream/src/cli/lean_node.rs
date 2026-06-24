use std::{net::IpAddr, path::PathBuf};

use clap::{Parser, error::ErrorKind};
use ream_fork_choice_lean::store::BlockProductionStrategy;
use ream_network_spec::{cli::lean_network_parser, networks::LeanNetworkSpec};
use ream_p2p::bootnodes::Bootnodes;
use url::Url;

use crate::cli::constants::{
    DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_HTTP_PORT, DEFAULT_METRICS_ADDRESS,
    DEFAULT_METRICS_ENABLED, DEFAULT_METRICS_PORT, DEFAULT_SOCKET_ADDRESS, DEFAULT_SOCKET_PORT,
};

#[derive(Debug, Parser, Clone)]
pub struct LeanNodeConfig {
    #[arg(
      long,
      help = "Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network",
      value_parser = lean_network_parser
  )]
    pub network: LeanNetworkSpec,

    #[arg(
        default_value = "default",
        long,
        help = "Bootnodes configuration: Use 'default' for network defaults, 'none' to disable, '/path/to/nodes.yaml' for a YAML file with ENRs, or comma-delimited base64-encoded ENRs"
    )]
    pub bootnodes: Bootnodes,

    #[arg(long, help = "HTTP URL of a remote node to sync checkpoint state from")]
    pub checkpoint_sync_url: Option<Url>,

    #[arg(long, help = "The path to the validator registry")]
    pub validator_registry_path: PathBuf,

    #[arg(
        default_value = "ream_0",
        long,
        help = "Node identifier for validator registry (e.g., 'ream_0', 'zeam_0')"
    )]
    pub node_id: String,

    #[arg(
        long,
        help = "The path to the hex encoded secp256k1 libp2p key",
        alias = "node-key"
    )]
    pub private_key_path: Option<PathBuf>,

    #[arg(long, help = "Set P2P socket address", default_value_t = DEFAULT_SOCKET_ADDRESS)]
    pub socket_address: IpAddr,

    #[arg(long, help = "Set P2P socket port (QUIC)", default_value_t = DEFAULT_SOCKET_PORT)]
    pub socket_port: u16,

    #[arg(long, help = "Set HTTP address", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port", default_value_t = DEFAULT_HTTP_PORT)]
    pub http_port: u16,

    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,

    #[arg(long = "metrics", help = "Enable metrics", default_value_t = DEFAULT_METRICS_ENABLED)]
    pub enable_metrics: bool,

    #[arg(long, help = "Set metrics address", default_value_t = DEFAULT_METRICS_ADDRESS)]
    pub metrics_address: IpAddr,

    #[arg(long, help = "Set metrics port", default_value_t = DEFAULT_METRICS_PORT)]
    pub metrics_port: u16,

    #[arg(
        long,
        help = "Set node as aggregator for committee signature aggregation",
        default_value_t = false
    )]
    pub is_aggregator: bool,

    #[arg(
        long,
        value_delimiter = ',',
        requires = "is_aggregator",
        help = "Additional attestation subnet ids to subscribe to and aggregate from (comma-separated, e.g. '0,3,7'). Requires --is-aggregator."
    )]
    pub aggregate_subnet_ids: Vec<u64>,

    #[arg(
        long,
        default_value_t = 1,
        value_parser = clap::value_parser!(u64).range(1..),
        help = "Number of attestation committees (subnets). Each validator's subnet is `validator_id % count`."
    )]
    pub attestation_committee_count: u64,

    #[arg(
        long,
        default_value = "round-based",
        value_parser = block_production_parser,
        help = "Attestation selection strategy for block production: round-based or tiered."
    )]
    pub block_production: BlockProductionStrategy,
}

impl LeanNodeConfig {
    pub fn validate(&self) -> Result<(), clap::Error> {
        for subnet_id in &self.aggregate_subnet_ids {
            if *subnet_id >= self.attestation_committee_count {
                return Err(clap::Error::raw(
                    ErrorKind::ValueValidation,
                    format!(
                        "--aggregate-subnet-ids contains {subnet_id}, but only {} attestation subnets exist",
                        self.attestation_committee_count
                    ),
                ));
            }
        }

        Ok(())
    }
}

fn block_production_parser(value: &str) -> Result<BlockProductionStrategy, String> {
    match value {
        "round-based" => Ok(BlockProductionStrategy::RoundBased),
        "tiered" => Ok(BlockProductionStrategy::Tiered),
        other => Err(format!("expected 'round-based' or 'tiered', got '{other}'")),
    }
}

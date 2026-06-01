use std::{net::IpAddr, path::PathBuf};

use alloy_primitives::B256;
use clap::Parser;
use libp2p::Multiaddr;
use ream_p2p::bootnodes::Bootnodes;

#[derive(Parser, Debug)]
#[command(
    name = "ream-da",
    about = "Ream PeerDAS Data Availability Node — custodies and serves all 128 data columns",
    version
)]
pub struct Cli {
    /// URL of the beacon node's Beacon API (for canonical head events)
    #[arg(long, default_value = "http://localhost:5052")]
    pub beacon_url: String,

    /// Directory for storing column sidecar data and slot index
    #[arg(long, default_value = "./da-data")]
    pub data_dir: PathBuf,

    /// Ethereum network to connect to
    #[arg(long, default_value = "mainnet")]
    pub network: String,

    /// IP address to listen on for P2P connections
    #[arg(long, default_value = "0.0.0.0")]
    pub p2p_host: IpAddr,

    /// TCP port for libp2p connections
    #[arg(long, default_value_t = 9100)]
    pub p2p_port: u16,

    /// UDP port for discv5 discovery
    #[arg(long, default_value_t = 9101)]
    pub discovery_port: u16,

    /// Bootnodes to connect to on startup
    /// Accepts: "default", "none", comma-separated ENRs, or path to YAML file
    #[arg(long, default_value = "default")]
    pub bootnodes: Bootnodes,

    /// Direct libp2p multiaddr peers to dial on startup, bypassing discv5
    /// e.g. /ip4/172.16.0.13/tcp/33000/p2p/16Uiu2HAmVok6ZGmqT9dbBmQzSxmiy8ASE8hwEBKcGBJxfkxST6fm
    #[arg(long, value_delimiter = ',')]
    pub static_peers: Vec<Multiaddr>,

    /// Disable peer discovery (useful for local testing)
    #[arg(long, default_value_t = false)]
    pub disable_discovery: bool,

    /// Optional genesis validators root to use for network spec (overrides default for selected network)
    #[arg(long)]
    pub genesis_validators_root: Option<B256>,
}

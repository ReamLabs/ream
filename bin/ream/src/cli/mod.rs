pub mod beacon_node;
pub mod constants;
pub mod validator_node;

use clap::{Parser, Subcommand};
use ream_node::version::FULL_VERSION;

use crate::cli::{beacon_node::BeaconNodeConfig, validator_node::ValidatorNodeConfig};

#[derive(Debug, Parser)]
#[command(author, version = FULL_VERSION, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the node
    #[command(name = "beacon_node")]
    BeaconNode(BeaconNodeConfig),
    #[command(name = "validator_node")]
    ValidatorNode(ValidatorNodeConfig),
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        time::Duration,
    };

    use ream_network_spec::networks::Network;
    use url::Url;

    use super::*;
    use crate::cli::constants::DEFAULT_BEACON_API_ENDPOINT;

    #[test]
    fn test_cli_beacon_node_command() {
        let cli = Cli::parse_from([
            "program",
            "beacon_node",
            "--verbosity",
            "2",
            "--socket-address",
            "127.0.0.1",
            "--socket-port",
            "9001",
            "--discovery-port",
            "9002",
        ]);

        match cli.command {
            Commands::BeaconNode(config) => {
                assert_eq!(config.network.network, Network::Mainnet);
                assert_eq!(config.verbosity, 2);
                assert_eq!(
                    config.socket_address,
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
                );
                assert_eq!(config.socket_port, 9001);
                assert_eq!(config.discovery_port, 9002);
            }
            _ => unreachable!("This test should only validate the beacon node cli"),
        }
    }

    #[test]
    fn test_cli_validator_node_command() {
        let cli = Cli::parse_from([
            "program",
            "validator_node",
            "--verbosity",
            "2",
            "--beacon-api-endpoint",
            "http://localhost:5052",
            "--request-timeout",
            "3",
        ]);

        match cli.command {
            Commands::ValidatorNode(config) => {
                assert_eq!(config.verbosity, 2);
                assert_eq!(
                    config.beacon_api_endpoint,
                    Url::parse(DEFAULT_BEACON_API_ENDPOINT).expect("Invalid URL")
                );
                assert_eq!(config.request_timeout, Duration::from_secs(3));
            }
            _ => unreachable!("This test should only validate the validator node cli"),
        }
    }
}

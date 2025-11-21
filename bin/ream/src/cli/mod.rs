pub mod account_manager;
pub mod beacon_node;
pub mod constants;
pub mod generate_private_key;
pub mod import_keystores;
pub mod lean_node;
pub mod validator_node;
pub mod verbosity;
pub mod voluntary_exit;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use ream_node::version::FULL_VERSION;

use crate::cli::{
    account_manager::AccountManagerConfig,
    beacon_node::BeaconNodeConfig,
    generate_private_key::GeneratePrivateKeyConfig,
    lean_node::LeanNodeConfig,
    validator_node::ValidatorNodeConfig,
    verbosity::{Verbosity, verbosity_parser},
    voluntary_exit::VoluntaryExitConfig,
};

#[derive(Debug, Parser)]
#[command(author, version = FULL_VERSION, about, long_about = None)]
pub struct Cli {
    /// Verbosity level (1=error, 2=warn, 3=info, 4=debug, 5=trace)
    #[arg(short, long, default_value = "3", value_parser = verbosity_parser)]
    pub verbosity: Verbosity,

    #[command(subcommand)]
    pub command: Commands,

    #[arg(
        long,
        help = "The directory for storing application data. If used together with --ephemeral, new child directory will be created."
    )]
    pub data_dir: Option<PathBuf>,

    #[arg(
        long,
        short,
        help = "Use new data directory, located in OS temporary directory. If used together with --data-dir, new directory will be created there instead."
    )]
    pub ephemeral: bool,

    #[arg(long, help = "Purges the database.")]
    pub purge_db: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the lean node
    #[command(name = "lean_node")]
    LeanNode(Box<LeanNodeConfig>),

    /// Start the beacon node
    #[command(name = "beacon_node")]
    BeaconNode(Box<BeaconNodeConfig>),

    /// Start the validator node
    #[command(name = "validator_node")]
    ValidatorNode(Box<ValidatorNodeConfig>),

    /// Manage validator accounts
    #[command(name = "account_manager")]
    AccountManager(Box<AccountManagerConfig>),

    /// Perform voluntary exit for a validator
    #[command(name = "voluntary_exit")]
    VoluntaryExit(Box<VoluntaryExitConfig>),

    /// Generate a secp256k1 keypair for lean node
    #[command(name = "generate_private_key")]
    GeneratePrivateKey(Box<GeneratePrivateKeyConfig>),
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
    fn test_cli_lean_node_command() {
        let cli = Cli::parse_from([
            "program",
            "--verbosity",
            "5",
            "lean_node",
            "--network",
            "./assets/lean/sample_spec.yml",
            "--validator-registry-path",
            "./assets/lean/validator_registry.yml",
            // Test for alias of `private-key-path`
            "--node-key",
            "awesome-node0.key",
        ]);

        assert_eq!(cli.verbosity, Verbosity::Trace);

        match cli.command {
            Commands::LeanNode(config) => {
                assert_eq!(
                    config.validator_registry_path.to_str().unwrap(),
                    "./assets/lean/validator_registry.yml"
                );

                // Verify the network spec was loaded from the YAML file (sample_spec.yml)
                assert_eq!(config.network.seconds_per_slot, 12);
                assert_eq!(config.network.genesis_time, 0);
                assert_eq!(config.network.justification_lookback_slots, 3);
                assert_eq!(config.network.num_validators, 4);

                assert_eq!(
                    config.private_key_path.as_ref().unwrap().to_str().unwrap(),
                    "awesome-node0.key"
                );
            }
            _ => unreachable!("This test should only validate the lean node cli"),
        }
    }

    #[test]
    fn test_cli_beacon_node_command() {
        let cli = Cli::parse_from([
            "program",
            "--verbosity",
            "2",
            "beacon_node",
            "--socket-address",
            "127.0.0.1",
            "--socket-port",
            "9001",
            "--discovery-port",
            "9002",
        ]);

        assert_eq!(cli.verbosity, Verbosity::Warn);

        match cli.command {
            Commands::BeaconNode(config) => {
                assert_eq!(config.network.network, Network::Mainnet);
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
            "--verbosity",
            "3",
            "validator_node",
            "--beacon-api-endpoint",
            "http://localhost:5052",
            "--request-timeout",
            "3",
            "--import-keystores",
            "./assets/keystore_dir/",
            "--suggested-fee-recipient",
            "0x003Fb16e421E42084EBC54bcdc7F0fa344cF9316",
            "--password",
            "𝔱𝔢𝔰𝔱𝔭𝔞𝔰𝔰𝔴𝔬𝔯𝔡🔑", // Taken directly from EIP-2335's test keystores
        ]);

        assert_eq!(cli.verbosity, Verbosity::Info);

        match cli.command {
            Commands::ValidatorNode(config) => {
                assert_eq!(
                    config.beacon_api_endpoint,
                    Url::parse(DEFAULT_BEACON_API_ENDPOINT).expect("Invalid URL")
                );
                assert_eq!(config.request_timeout, Duration::from_secs(3));
            }
            _ => unreachable!("This test should only validate the validator node cli"),
        }
    }

    #[test]
    fn test_cli_account_manager_command() {
        let cli = Cli::parse_from([
            "program",
            "--verbosity",
            "5",
            "account_manager",
            "--lifetime",
            "30",
            "--chunk-size",
            "10",
            "--activation-epoch",
            "100",
            "--num-active-epochs",
            "5",
        ]);

        assert_eq!(cli.verbosity, Verbosity::Trace);

        match cli.command {
            Commands::AccountManager(config) => {
                assert_eq!(config.lifetime, 30);
                assert_eq!(config.chunk_size, 10);
                assert_eq!(config.activation_epoch, 100);
                assert_eq!(config.num_active_epochs, 5);
            }
            _ => unreachable!("This test should only validate the account manager cli"),
        }
    }

    #[test]
    fn test_verbosity_levels() {
        // Test error level (1)
        let cli = Cli::parse_from(["program", "--verbosity", "1", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Error);

        // Test warn level (2)
        let cli = Cli::parse_from(["program", "--verbosity", "2", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Warn);

        // Test info level (3)
        let cli = Cli::parse_from(["program", "--verbosity", "3", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Info);

        // Test debug level (4)
        let cli = Cli::parse_from(["program", "--verbosity", "4", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Debug);

        // Test trace level (5)
        let cli = Cli::parse_from(["program", "--verbosity", "5", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Trace);

        // Test default verbosity (should be 3 which maps to info)
        let cli = Cli::parse_from(["program", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Info);

        // Test short flag -v
        let cli = Cli::parse_from(["program", "-v", "1", "beacon_node"]);
        assert_eq!(cli.verbosity, Verbosity::Error);

        // Test invalid verbosity level (0)
        let result = Cli::try_parse_from(["program", "--verbosity", "0", "beacon_node"]);
        assert!(result.is_err());

        // Test invalid verbosity level (6)
        let result = Cli::try_parse_from(["program", "--verbosity", "6", "beacon_node"]);
        assert!(result.is_err());
    }
}

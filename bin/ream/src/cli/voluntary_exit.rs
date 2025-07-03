use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use ream_network_spec::{cli::network_parser, networks::NetworkSpec};
use url::Url;

use crate::cli::{
    constants::{DEFAULT_BEACON_API_ENDPOINT, DEFAULT_NETWORK, DEFAULT_REQUEST_TIMEOUT},
    validator_node::duration_parser,
};

#[derive(Debug, Parser)]
pub struct VoluntaryExitConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(long, help = "Set HTTP url of the beacon api endpoint", default_value = DEFAULT_BEACON_API_ENDPOINT)]
    pub beacon_api_endpoint: Url,

    #[arg(long, help = "Set HTTP request timeout for beacon api calls", default_value = DEFAULT_REQUEST_TIMEOUT, value_parser = duration_parser)]
    pub request_timeout: Duration,

    #[arg(
        long,
        help = "Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file",
        default_value = DEFAULT_NETWORK,
        value_parser = network_parser
    )]
    pub network: Arc<NetworkSpec>,

    #[arg(long, help = "The directory for importing keystores")]
    pub import_keystores: PathBuf,

    #[arg(
        long,
        group = "password_source",
        help = "The plaintext password file to use for keystores"
    )]
    pub password_file: Option<PathBuf>,

    #[arg(
        long,
        group = "password_source",
        help = "The password to use for keystores. It's recommended to use password-file over this in order to prevent your keystore password from appearing in the shell history"
    )]
    pub password: Option<String>,

    #[arg(long, help = "The validator index to exit")]
    pub validator_index: u64,

    #[arg(long, help = "The epoch for the voluntary exit")]
    pub epoch: u64,

    #[arg(long, help = "Wait until the validator has fully exited")]
    pub wait: bool,
}

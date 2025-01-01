use clap::Parser;
use ream::cli::{Cli, Commands};
use tracing::info;
use tracing_subscriber::EnvFilter;

fn main() {
    // Set the default log level to `info` if not set
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Node(_cmd) => {
            info!("Starting node");
        }
    }
}

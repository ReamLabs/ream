use clap::Parser;
use tracing::{info, Level};

use ream::cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Node(cmd) => {
            let level = match cmd.verbosity {
                0 => Level::ERROR,
                1 => Level::WARN,
                2 => Level::INFO,
                3 => Level::DEBUG,
                _ => Level::TRACE,
            };
            tracing_subscriber::fmt().with_max_level(level).init();

            info!("Starting node with verbosity level {:?}", level);
        }
    }
}

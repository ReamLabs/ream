use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct GenerateNetworkKeyConfig {
    #[arg(long, help = "Output path for the generated network key")]
    pub output_path: PathBuf,
}

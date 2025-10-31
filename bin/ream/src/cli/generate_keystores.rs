use std::{fs, path::PathBuf};

use clap::Parser;
use ream_keystore::lean_keystore::generate_keystore;

#[derive(Debug, Parser)]
pub struct GenerateKeystoreConfig {
    #[arg(long, default_value = "keystore-hashsig.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value_t = 1)]
    pub number_of_validators: u64,

    #[arg(long, default_value_t = 1)]
    pub number_of_keys: usize,
}

pub fn run_generate_keystore(keystore_config: GenerateKeystoreConfig) -> anyhow::Result<()> {
    fs::write(
        keystore_config.output,
        serde_yaml::to_string(&generate_keystore(
            keystore_config.number_of_validators,
            keystore_config.number_of_keys,
        )?)?,
    )?;

    Ok(())
}

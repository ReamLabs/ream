use std::{fs, path::PathBuf};

use clap::Parser;
use ream_keystore::lean_keystore::ValidatorRegistry;

#[derive(Debug, Parser)]
pub struct GenerateValidatorRegistryConfig {
    #[arg(long, default_value = "validator_registry.yaml")]
    pub output: PathBuf,

    #[arg(long, default_value_t = 1)]
    pub number_of_nodes: u64,

    #[arg(long, default_value_t = 1)]
    pub number_of_validators_per_node: u64,
}

pub fn run_generate_validator_registry(
    keystore_config: GenerateValidatorRegistryConfig,
) -> anyhow::Result<()> {
    fs::write(
        keystore_config.output,
        serde_yaml::to_string(&ValidatorRegistry::new(
            keystore_config.number_of_nodes,
            keystore_config.number_of_validators_per_node,
        )?)?,
    )?;

    Ok(())
}

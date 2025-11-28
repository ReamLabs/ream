use std::{
    collections::HashMap,
    fs::{self, create_dir_all},
    path::PathBuf,
};

use anyhow::ensure;
use clap::Parser;
use rand::rng;
use ream_keystore::lean_keystore::{
    ConfigFile, ValidatorKeysManifest, ValidatorKeystoreRaw, ValidatorRegistry,
};
use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, public_key::PublicKey};

const NUM_ACTIVE_EPOCHS: u64 = 262144;

#[derive(Debug, Parser)]
pub struct GenerateValidatorRegistryConfig {
    #[arg(long, default_value = ".", help = "Must be a path, not a file name")]
    pub output: PathBuf,

    #[arg(long, default_value_t = 1)]
    pub number_of_nodes: u64,

    #[arg(long, default_value_t = 1)]
    pub number_of_validators_per_node: u64,
}

pub fn run_generate_validator_registry(
    keystore_config: GenerateValidatorRegistryConfig,
) -> anyhow::Result<()> {
    ensure!(
        !keystore_config.output.is_file(),
        "Output must be a directory path"
    );
    create_dir_all(&keystore_config.output)?;

    let mut rng = rng();
    let mut validator_registry = HashMap::new();
    let mut validator_index = 0;
    for node_index in 0..keystore_config.number_of_nodes {
        let mut validator_ids = vec![];
        for _ in 0..keystore_config.number_of_validators_per_node {
            validator_ids.push(validator_index);
            validator_index += 1
        }
        validator_registry.insert(format!("ream_{node_index}"), validator_ids);
    }

    let mut path = keystore_config.output;
    path.push("validators.yaml");
    fs::write(
        &path,
        serde_yaml::to_string(&ValidatorRegistry {
            nodes: validator_registry,
        })?,
    )?;
    path.pop();

    path.push("hash-sig-keys");
    create_dir_all(&path)?;
    let mut validators: Vec<ValidatorKeystoreRaw> = Vec::new();
    let mut genesis_validators: Vec<PublicKey> = vec![];
    for i in 0..(keystore_config.number_of_nodes * keystore_config.number_of_validators_per_node) {
        let (public_key, private_key) =
            PrivateKey::generate_key_pair(&mut rng, 0, NUM_ACTIVE_EPOCHS as usize);
        genesis_validators.push(public_key);

        let filename: String = format!("validator_{i}_sk.json");
        path.push(&filename);
        fs::write(&path, serde_json::to_string(&private_key.inner)?)?;
        path.pop();

        validators.push(ValidatorKeystoreRaw {
            index: i,
            public_key,
            privkey_file: filename,
        });
    }

    path.push("validator-keys-manifest.yaml");
    fs::write(
        &path,
        serde_yaml::to_string(&ValidatorKeysManifest {
            key_scheme: "SIGTopLevelTargetSumLifetime32Dim64Base8".to_string(),
            hash_function: "Poseidon2".to_string(),
            encoding: "TargetSum".to_string(),
            lifetime: 4294967296u64,
            log_num_active_epochs: 18,
            num_active_epochs: NUM_ACTIVE_EPOCHS,
            num_validators: validators.len() as u64,
            validators,
        })?,
    )?;

    path.pop();
    path.pop();
    path.push("config.yaml");
    fs::write(
        &path,
        serde_yaml::to_string(&ConfigFile {
            genesis_time: 1704085200,
            num_validators: genesis_validators.len() as u64,
            genesis_validators,
        })?,
    )?;

    Ok(())
}

use std::{fs, path::Path};

use anyhow::anyhow;
use ream_consensus_lean::validator::Validator;
use ream_keystore::lean_keystore::{ValidatorKeysManifest, ValidatorKeystore, ValidatorRegistry};
use ream_post_quantum_crypto::hashsig::private_key::{HashSigPrivateKey, PrivateKey};

/// Load validator registry from YAML file for a specific node
///
/// # Arguments
/// * `path` - Path to the validator registry YAML file
/// * `node_id` - Node identifier (e.g., "ream_0", "zeam_0")
pub fn load_validator_registry<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
    node_id: &str,
) -> anyhow::Result<Vec<ValidatorKeystore>> {
    let mut path = path.as_ref().to_path_buf();
    let validator_registry_yaml = fs::read_to_string(&path)
        .map_err(|err| anyhow!("Failed to read validator registry file {err}"))?;
    let validator_registry = serde_yaml::from_str::<ValidatorRegistry>(&validator_registry_yaml)
        .map_err(|err| anyhow!("Failed to parse validator registry YAML: {err}"))?;

    path.pop();
    path.push("hash-sig-keys/");
    let mut validator_keystores = vec![];
    for ream_validator_index in validator_registry.nodes.get(node_id).expect("") {
        path.push("validator-keys-manifest.yaml");

        let validator_keys_manifest_yaml = fs::read_to_string(&path)
            .map_err(|err| anyhow!("Failed to read validator keys manifest yaml file {err}",))?;

        let validator_keys_manifest =
            serde_yaml::from_str::<ValidatorKeysManifest>(&validator_keys_manifest_yaml)
                .map_err(|err| anyhow!("Failed to parse validator keys manifest yaml: {err}"))?;

        let validator = validator_keys_manifest
            .validators
            .get(*ream_validator_index as usize)
            .expect("Failed to get ream validator index");

        path.pop();
        path.push(validator.privkey_file.clone());
        let validator_private_key_json = fs::read_to_string(&path)
            .map_err(|err| anyhow!("Failed to read validator private key json file {err}",))?;
        let hash_sig_private_key =
            serde_json::from_str::<HashSigPrivateKey>(&validator_private_key_json)
                .map_err(|err| anyhow!("Failed to parse validator private key json: {err}"))?;
        let private_key = PrivateKey::new(hash_sig_private_key);

        validator_keystores.push(ValidatorKeystore {
            index: *ream_validator_index,
            public_key: validator.public_key.clone(),
            private_key,
        });
        path.pop();
    }
    Ok(validator_keystores)
}

pub fn load_validator_public_keys<P: AsRef<Path> + std::fmt::Debug>(
    path: P,
) -> anyhow::Result<Vec<Validator>> {
    let validator_keys_manifest_yaml = fs::read_to_string(&path)
        .map_err(|err| anyhow!("Failed to read validator keys manifest yaml file {err}",))?;

    let validator_keys_manifest =
        serde_yaml::from_str::<ValidatorKeysManifest>(&validator_keys_manifest_yaml)
            .map_err(|err| anyhow!("Failed to parse validator keys manifest yaml: {err}"))?;

    let mut validator_public_keys = vec![];
    for validator in validator_keys_manifest.validators {
        validator_public_keys.push(Validator {
            public_key: validator.public_key,
        });
    }
    Ok(validator_public_keys)
}

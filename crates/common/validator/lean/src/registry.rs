use std::{fs, path::Path};

use anyhow::anyhow;
use ream_keystore::lean_keystore::{ValidatorKeysManifest, ValidatorKeystore, ValidatorRegistry};
use ream_post_quantum_crypto::leansig::private_key::{LeanSigPrivateKey, PrivateKey};

enum PrivateKeyFormat {
    Json,
    Ssz,
}

impl TryFrom<&str> for PrivateKeyFormat {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "json" => Ok(PrivateKeyFormat::Json),
            "ssz" => Ok(PrivateKeyFormat::Ssz),
            _ => Err(anyhow!("Unsupported private key format: {value}")),
        }
    }
}

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
    for ream_validator_index in validator_registry
        .nodes
        .get(node_id)
        .ok_or_else(|| anyhow!("Failed to get validator indexes for given node ID {node_id}"))?
    {
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
        let private_key = match PrivateKeyFormat::try_from(
            path.extension().and_then(|s| s.to_str()).unwrap_or(""),
        )? {
            PrivateKeyFormat::Json => PrivateKey::new(
                serde_json::from_str::<LeanSigPrivateKey>(&fs::read_to_string(&path)?)
                    .map_err(|err| anyhow!("Failed to parse validator private key json: {err}"))?,
            ),
            PrivateKeyFormat::Ssz => {
                PrivateKey::from_bytes(&fs::read(&path).map_err(|err| {
                    anyhow!("Failed to read validator private key ssz file {err}",)
                })?)?
            }
        };

        validator_keystores.push(ValidatorKeystore {
            index: *ream_validator_index,
            public_key: validator.public_key,
            private_key,
        });
        path.pop();
    }
    Ok(validator_keystores)
}

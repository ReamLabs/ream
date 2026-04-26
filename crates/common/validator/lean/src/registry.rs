use std::{fmt::Debug, fs, path::Path};

use anyhow::{anyhow, bail};
use ream_keystore::lean_keystore::{AnnotatedValidatorRegistry, ValidatorKeystore};
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

/// Load validator registry from annotated validators YAML file for a specific node.
///
/// The annotated format includes public key and private key file references inline,
/// eliminating the need for a separate validator-keys-manifest.yaml lookup.
///
/// # Arguments
/// * `path` - Path to the annotated validators YAML file
/// * `node_id` - Node identifier (e.g., "ream_0", "zeam_0")
pub fn load_validator_registry<P: AsRef<Path> + Debug>(
    path: P,
    node_id: &str,
) -> anyhow::Result<Vec<ValidatorKeystore>> {
    let path = path.as_ref().to_path_buf();
    let validator_registry_yaml = fs::read_to_string(&path)
        .map_err(|err| anyhow!("Failed to read validator registry file {err}"))?;
    let registry = serde_yaml::from_str::<AnnotatedValidatorRegistry>(&validator_registry_yaml)
        .map_err(|err| anyhow!("Failed to parse annotated validator registry YAML: {err}"))?;

    let keys_dir = path
        .parent()
        .ok_or_else(|| anyhow!("Invalid registry path"))?
        .join("hash-sig-keys");

    let entries = registry
        .nodes
        .get(node_id)
        .ok_or_else(|| anyhow!("Failed to get validator entries for given node ID {node_id}"))?;

    let mut validator_keystores = vec![];

    // In devnet4 dual-key mode, each validator index has two consecutive entries:
    // first the attester key, then the proposer key.
    let mut iter = entries.iter();
    while let Some(attester_entry) = iter.next() {
        let proposer_entry = iter.next().ok_or_else(|| {
            anyhow!(
                "Missing proposer entry for validator index {}",
                attester_entry.index
            )
        })?;
        if attester_entry.index != proposer_entry.index {
            bail!(
                "Mismatched validator indices: attester={} proposer={}",
                attester_entry.index,
                proposer_entry.index
            );
        }

        let attestation_private_key =
            load_private_key(&keys_dir.join(&attester_entry.private_key_file))?;
        let proposal_private_key =
            load_private_key(&keys_dir.join(&proposer_entry.private_key_file))?;

        validator_keystores.push(ValidatorKeystore {
            index: attester_entry.index,
            attestation_public_key: attester_entry.public_key_hex,
            proposal_public_key: proposer_entry.public_key_hex,
            attestation_private_key,
            proposal_private_key,
        });
    }

    Ok(validator_keystores)
}

fn load_private_key(path: &Path) -> anyhow::Result<PrivateKey> {
    match PrivateKeyFormat::try_from(
        path.extension()
            .and_then(|string| string.to_str())
            .unwrap_or("Failed to get private key format"),
    )? {
        PrivateKeyFormat::Json => Ok(PrivateKey::new(
            serde_json::from_str::<LeanSigPrivateKey>(&fs::read_to_string(path)?)
                .map_err(|err| anyhow!("Failed to parse validator private key json: {err}"))?,
        )),
        PrivateKeyFormat::Ssz => {
            Ok(PrivateKey::from_bytes(&fs::read(path).map_err(|err| {
                anyhow!("Failed to read validator private key ssz file {err}")
            })?)?)
        }
    }
}

use std::{fmt::Debug, fs, path::Path};

use anyhow::{anyhow, bail};
use ream_keystore::lean_keystore::{AnnotatedValidatorRegistry, ValidatorKeystore};
use ream_network_spec::networks::GenesisValidatorEntry;
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
/// Each validator's attestation and proposal public keys must differ, and must equal its
/// entry in the genesis validators, so a misconfigured manifest stops the node before it
/// signs anything.
///
/// # Arguments
/// * `path` - Path to the annotated validators YAML file
/// * `node_id` - Node identifier (e.g., "ream_0", "zeam_0")
/// * `genesis_validators` - The genesis validator registry the keys must match
pub fn load_validator_registry<P: AsRef<Path> + Debug>(
    path: P,
    node_id: &str,
    genesis_validators: &[GenesisValidatorEntry],
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

    // In dual-key mode, each validator index has two consecutive entries:
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

        // A validator signs a proposal and an attestation in the same slot, and XMSS is a
        // stateful one-time signature, so one key shared by both roles would be reused.
        if attester_entry.public_key_hex == proposer_entry.public_key_hex {
            bail!(
                "Attestation and proposal keys must differ for validator {}, but the manifest assigns the same key to both",
                attester_entry.index
            );
        }

        let genesis_entry = usize::try_from(attester_entry.index)
            .ok()
            .and_then(|index| genesis_validators.get(index))
            .ok_or_else(|| {
                anyhow!(
                    "Validator {} is not in the genesis registry of {} validators",
                    attester_entry.index,
                    genesis_validators.len()
                )
            })?;
        if attester_entry.public_key_hex.inner != genesis_entry.attestation_public_key {
            bail!(
                "Attestation public key for validator {} does not match genesis",
                attester_entry.index
            );
        }
        if proposer_entry.public_key_hex.inner != genesis_entry.proposal_public_key {
            bail!(
                "Proposal public key for validator {} does not match genesis",
                attester_entry.index
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs};

    use ream_keystore::lean_keystore::{AnnotatedValidatorEntry, AnnotatedValidatorRegistry};
    use ream_network_spec::networks::GenesisValidatorEntry;
    use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, public_key::PublicKey};

    use super::load_validator_registry;

    /// Writes one validator's manifest entries, with `private_key` as both key files, and
    /// loads them against `genesis_validators`.
    fn load(
        attestation_public_key: PublicKey,
        proposal_public_key: PublicKey,
        private_key: &[u8],
        genesis_validators: &[GenesisValidatorEntry],
    ) -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let keys_dir = dir.path().join("hash-sig-keys");
        fs::create_dir(&keys_dir)?;
        fs::write(keys_dir.join("attester.ssz"), private_key)?;
        fs::write(keys_dir.join("proposer.ssz"), private_key)?;

        let registry = AnnotatedValidatorRegistry {
            nodes: HashMap::from([(
                "ream_0".to_string(),
                vec![
                    AnnotatedValidatorEntry {
                        index: 0,
                        public_key_hex: attestation_public_key,
                        private_key_file: "attester.ssz".to_string(),
                    },
                    AnnotatedValidatorEntry {
                        index: 0,
                        public_key_hex: proposal_public_key,
                        private_key_file: "proposer.ssz".to_string(),
                    },
                ],
            )]),
        };
        let path = dir.path().join("annotated_validators.yaml");
        fs::write(&path, serde_yaml::to_string(&registry)?)?;

        load_validator_registry(&path, "ream_0", genesis_validators).map(|_| ())
    }

    #[test]
    fn test_load_validator_registry_checks_keys_against_genesis() {
        // Key generation is slow, so two key pairs cover every case.
        let (attestation, private_key) = PrivateKey::generate_key_pair(0, 8);
        let (proposal, _) = PrivateKey::generate_key_pair(0, 8);
        let private_key = private_key.to_bytes();
        let genesis = |attestation: PublicKey, proposal: PublicKey| {
            vec![GenesisValidatorEntry {
                attestation_public_key: attestation.inner,
                proposal_public_key: proposal.inner,
            }]
        };

        let cases = [
            (
                "matching keys",
                attestation,
                proposal,
                genesis(attestation, proposal),
                None,
            ),
            (
                "same key for both roles",
                attestation,
                attestation,
                genesis(attestation, attestation),
                Some("must differ"),
            ),
            (
                "swapped roles",
                proposal,
                attestation,
                genesis(attestation, proposal),
                Some("Attestation public key for validator 0 does not match genesis"),
            ),
            (
                "proposal key not in genesis",
                attestation,
                proposal,
                genesis(attestation, attestation),
                Some("Proposal public key for validator 0 does not match genesis"),
            ),
            (
                "index past the genesis registry",
                attestation,
                proposal,
                vec![],
                Some("not in the genesis registry"),
            ),
        ];

        for (name, attestation_key, proposal_key, genesis_validators, want_err) in cases {
            let result = load(
                attestation_key,
                proposal_key,
                &private_key,
                &genesis_validators,
            );
            match want_err {
                None => assert!(result.is_ok(), "{name}: unexpected error {result:?}"),
                Some(want) => {
                    let err = result.expect_err(name).to_string();
                    assert!(
                        err.contains(want),
                        "{name}: error {err:?} does not contain {want:?}"
                    );
                }
            }
        }
    }
}

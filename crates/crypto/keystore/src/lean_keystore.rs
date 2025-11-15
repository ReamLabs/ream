use std::collections::BTreeMap;

use rand::rng;
use ream_post_quantum_crypto::hashsig::{private_key::PrivateKey, public_key::PublicKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ValidatorKeystore {
    pub id: u64,
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

/// YAML structure for node-based validator mapping
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ValidatorRegistry {
    #[serde(flatten)]
    pub nodes: BTreeMap<String, Vec<ValidatorKeystore>>,
}

impl ValidatorRegistry {
    pub fn new(
        number_of_nodes: u64,
        number_of_validators_per_node: u64,
    ) -> anyhow::Result<ValidatorRegistry> {
        let mut rng = rng();
        let mut keystore = BTreeMap::new();
        let mut validator_index = 0;
        for node_index in 0..number_of_nodes {
            let mut validator_keystores = vec![];
            for _ in 0..number_of_validators_per_node {
                let (public_key, private_key) = PrivateKey::generate_key_pair(&mut rng, 0, 1);
                validator_keystores.push(ValidatorKeystore {
                    id: validator_index,
                    public_key,
                    private_key,
                });
                validator_index += 1
            }
            keystore.insert(format!("ream_{node_index}"), validator_keystores);
        }
        Ok(ValidatorRegistry { nodes: keystore })
    }
}

#[cfg(test)]
mod tests {
    use crate::lean_keystore::ValidatorRegistry;

    #[test]
    #[ignore = "slow"]
    fn test_generate_validator_registry() {
        let validator_registry = ValidatorRegistry::new(1, 4).unwrap();
        let encoded = serde_yaml::to_string(&validator_registry).unwrap();

        let decoded = serde_yaml::from_str::<ValidatorRegistry>(&encoded).unwrap();

        assert_eq!(validator_registry, decoded);
    }
}

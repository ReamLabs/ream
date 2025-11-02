use std::collections::BTreeMap;

use alloy_primitives::Bytes;
use rand::rng;
use ream_post_quantum_crypto::hashsig::private_key::PrivateKey;

#[derive(serde::Serialize)]
pub struct KeyPair {
    pub public_key: Bytes,
    pub private_key: Bytes,
}

pub fn generate_keystore(
    number_of_validators: u64,
    number_of_keys: usize,
) -> anyhow::Result<BTreeMap<String, Vec<KeyPair>>> {
    let mut keystore = BTreeMap::new();
    let mut range = rng();
    for index in 0..number_of_validators {
        let mut keys = vec![];
        for _ in 0..number_of_keys {
            let (public_key, private_key) = PrivateKey::generate_key_pair(&mut range, 0, 1);
            keys.push(KeyPair {
                public_key: public_key.to_bytes(),
                private_key: private_key.to_bytes(),
            });
        }
        keystore.insert(format!("validator_{index}"), keys);
    }
    Ok(keystore)
}

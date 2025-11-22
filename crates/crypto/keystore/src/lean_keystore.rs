use std::collections::HashMap;

use ream_post_quantum_crypto::hashsig::{private_key::PrivateKey, public_key::PublicKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeysManifest {
    pub key_scheme: String,
    pub hash_function: String,
    pub encoding: String,
    pub lifetime: u64,
    pub log_num_active_epochs: u64,
    pub num_active_epochs: u64,
    pub num_validators: u64,
    pub validators: Vec<ValidatorKeystoreRaw>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeystoreRaw {
    pub index: u64,
    #[serde(rename = "pubkey_hex")]
    pub public_key: PublicKey,
    pub privkey_file: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeystore {
    pub index: u64,
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

/// YAML structure for node-based validator mapping
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ValidatorRegistry {
    #[serde(flatten)]
    pub nodes: HashMap<String, Vec<u64>>,
}

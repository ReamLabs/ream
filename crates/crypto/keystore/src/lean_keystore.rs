use std::collections::HashMap;

#[cfg(feature = "devnet4")]
use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, public_key::PublicKey};
use serde::{Deserialize, Serialize};

#[cfg(feature = "devnet4")]
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

#[cfg(feature = "devnet4")]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeystoreRaw {
    pub index: u64,
    pub attestation_public_key_hex: PublicKey,
    pub proposal_public_key_hex: PublicKey,
    pub attestation_private_key_file: String,
    pub proposal_private_key_file: String,
}

#[cfg(feature = "devnet4")]
#[derive(Debug, PartialEq)]
pub struct ValidatorKeystore {
    pub index: u64,
    pub attestation_public_key: PublicKey,
    pub proposal_public_key: PublicKey,
    pub attestation_private_key: PrivateKey,
    pub proposal_private_key: PrivateKey,
}

/// YAML structure for node-based validator mapping
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ValidatorRegistry {
    #[serde(flatten)]
    pub nodes: HashMap<String, Vec<u64>>,
}

#[cfg(feature = "devnet4")]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub struct ConfigFile {
    pub genesis_time: u64,
    pub num_validators: u64,
    pub genesis_validators: Vec<GenesisValidatorEntry>,
}

/// A single validator's public keys in the genesis configuration.
#[cfg(feature = "devnet4")]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct GenesisValidatorEntry {
    pub attestation_public_key: PublicKey,
    pub proposal_public_key: PublicKey,
}

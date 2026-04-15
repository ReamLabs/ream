use std::collections::HashMap;

#[cfg(any(feature = "devnet3", feature = "devnet4"))]
use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, public_key::PublicKey};
use serde::{Deserialize, Serialize};

#[cfg(any(feature = "devnet3", feature = "devnet4"))]
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

#[cfg(feature = "devnet3")]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeystoreRaw {
    pub index: u64,
    #[serde(rename = "pubkey_hex")]
    pub public_key: PublicKey,
    #[serde(rename = "privkey_file")]
    pub private_key_file: String,
}

#[cfg(feature = "devnet4")]
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatorKeystoreRaw {
    pub index: u64,
    #[serde(
        rename = "attester_key_pubkey_hex",
        alias = "attestation_public_key_hex"
    )]
    pub attester_key_public_key_hex: PublicKey,
    #[serde(rename = "proposer_key_pubkey_hex", alias = "proposal_public_key_hex")]
    pub proposer_key_public_key_hex: PublicKey,
    #[serde(
        rename = "attester_key_privkey_file",
        alias = "attestation_private_key_file"
    )]
    pub attester_key_private_key_file: String,
    #[serde(
        rename = "proposer_key_privkey_file",
        alias = "proposal_private_key_file"
    )]
    pub proposer_key_private_key_file: String,
}

#[cfg(feature = "devnet3")]
#[derive(Debug, PartialEq)]
pub struct ValidatorKeystore {
    pub index: u64,
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
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

/// A single entry in the annotated validators YAML (includes public key and private key file path)
#[cfg(any(feature = "devnet3", feature = "devnet4"))]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AnnotatedValidatorEntry {
    pub index: u64,
    #[serde(rename = "pubkey_hex")]
    pub public_key_hex: PublicKey,
    #[serde(rename = "privkey_file")]
    pub private_key_file: String,
}

/// YAML structure for annotated validator registry (node -> list of annotated entries)
#[cfg(any(feature = "devnet3", feature = "devnet4"))]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AnnotatedValidatorRegistry {
    #[serde(flatten)]
    pub nodes: HashMap<String, Vec<AnnotatedValidatorEntry>>,
}

#[cfg(feature = "devnet3")]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub struct ConfigFile {
    pub genesis_time: u64,
    pub num_validators: u64,
    pub genesis_validators: Vec<PublicKey>,
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
    #[serde(alias = "attestation_pubkey")]
    pub attestation_public_key: PublicKey,
    #[serde(alias = "proposal_pubkey")]
    pub proposal_public_key: PublicKey,
}

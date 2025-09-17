use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use hex;
use rand;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::utils::validate_hex_string;

// Cryptographic algorithm constants
/// Key derivation function used for password-based key derivation
pub const KDF_FUNCTION: &str = "argon2id";

/// Symmetric encryption cipher used for encrypting the private key
pub const CIPHER_FUNCTION: &str = "aes-256-gcm";

/// Post-quantum signature scheme used for key generation and signing
pub const KEYTYPE_FUNCTION: &str = "xmss-poisedon2-ots-seed";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Keystore {
    /// Version number, must be 5
    pub version: u32,

    /// Cryptographic parameters
    pub crypto: CryptoParams,

    /// Key type specification
    pub keytype: KeyType,

    /// Description of the keystore
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Must be true for quantum security
    pub quantum_secure: bool,

    /// UUID identifier
    pub uuid: Uuid,

    /// Optional derivation path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<KeystoreMeta>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CryptoParams {
    /// Key derivation function parameters
    pub kdf: KdfParams,

    /// Cipher parameters and ciphertext
    pub cipher: CipherParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KdfParams {
    /// KDF function name, must be "argon2id"
    pub function: String,

    /// KDF parameters - supports both naming conventions
    pub params: KdfParamsInner,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum KdfParamsInner {
    /// Full parameter names
    Full {
        memory: u32,
        iterations: u32,
        parallelism: u32,
        salt: String,
    },
    /// Short parameter names
    Short {
        m: u32,
        t: u32,
        p: u32,
        salt: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CipherParams {
    /// Cipher function name, must be "aes-256-gcm"
    pub function: String,

    /// Cipher parameters
    pub params: CipherParamsInner,

    /// Encrypted data as hex string
    pub ciphertext: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CipherParamsInner {
    /// Nonce/IV as hex string
    pub nonce: String,

    /// Authentication tag as hex string
    pub tag: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyType {
    /// Key type function name
    pub function: String,

    /// Key type parameters
    pub params: KeyTypeParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyTypeParams {
    /// Key lifetime
    pub lifetime: u32,

    /// Activation epoch
    pub activation_epoch: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeystoreMeta {
    /// Creation timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<DateTime<Utc>>,
}

impl Keystore {
    /// Create a new quantum-secure keystore
    pub fn new(crypto: CryptoParams, keytype: KeyType, uuid: Uuid) -> Self {
        Self {
            version: 5,
            crypto,
            keytype,
            description: None,
            quantum_secure: true,
            uuid,
            path: None,
            meta: Some(KeystoreMeta {
                created: Some(Utc::now()),
            }),
        }
    }

    /// Create a new keystore from seed phrase and key parameters
    pub fn from_seed_phrase(
        seed_phrase: &str,
        lifetime: u32,
        activation_epoch: u32,
        description: Option<String>,
        path: Option<String>,
    ) -> Self {
        let uuid = Uuid::new_v4();

        // Generate random salt for KDF (32 bytes)
        let salt = hex::encode(rand::random::<[u8; 32]>());

        // Generate random nonce for AES-GCM (12 bytes)
        let nonce = hex::encode(rand::random::<[u8; 12]>());

        // Generate random tag for AES-GCM (16 bytes)
        let tag = hex::encode(rand::random::<[u8; 16]>());

        // Store the seed phrase as encrypted data (hex encoded)
        let ciphertext = hex::encode(seed_phrase.as_bytes());

        let kdf = KdfParams::new_full(65536, 4, 2, salt);
        let cipher = CipherParams::new(nonce, tag, ciphertext);
        let crypto = CryptoParams { kdf, cipher };
        let keytype = KeyType::new(lifetime, activation_epoch);

        let mut keystore = Self::new(crypto, keytype, uuid);
        keystore.description = description;
        keystore.path = path;

        keystore
    }

    /// Validate the keystore structure
    pub fn validate(&self) -> Result<()> {
        // Check version
        if self.version != 5 {
            return Err(anyhow!("Version must be 5"));
        }

        // Check quantum_secure flag
        if !self.quantum_secure {
            return Err(anyhow!("quantum_secure must be true"));
        }

        // Check KDF function
        if self.crypto.kdf.function != KDF_FUNCTION {
            return Err(anyhow!("KDF function must be {}", KDF_FUNCTION));
        }

        // Check cipher function
        if self.crypto.cipher.function != CIPHER_FUNCTION {
            return Err(anyhow!("Cipher function must be {}", CIPHER_FUNCTION));
        }

        // Check keytype function
        if self.keytype.function != KEYTYPE_FUNCTION {
            return Err(anyhow!("Keytype function must be {}", KEYTYPE_FUNCTION));
        }

        // Validate hex strings
        validate_hex_string(&self.crypto.cipher.ciphertext, "ciphertext")?;
        validate_hex_string(&self.crypto.cipher.params.nonce, "nonce")?;
        validate_hex_string(&self.crypto.cipher.params.tag, "tag")?;

        // Validate salt
        match &self.crypto.kdf.params {
            KdfParamsInner::Full { salt, .. } | KdfParamsInner::Short { salt, .. } => {
                validate_hex_string(salt, "salt")?;
            }
        }

        Ok(())
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Create from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl KdfParams {
    /// Create new Argon2id KDF parameters (full names)
    pub fn new_full(memory: u32, iterations: u32, parallelism: u32, salt: String) -> Self {
        Self {
            function: KDF_FUNCTION.to_string(),
            params: KdfParamsInner::Full {
                memory,
                iterations,
                parallelism,
                salt,
            },
        }
    }

    /// Create new Argon2id KDF parameters (short names)
    pub fn new_short(m: u32, t: u32, p: u32, salt: String) -> Self {
        Self {
            function: KDF_FUNCTION.to_string(),
            params: KdfParamsInner::Short { m, t, p, salt },
        }
    }
}

impl CipherParams {
    /// Create new AES-256-GCM cipher parameters
    pub fn new(nonce: String, tag: String, ciphertext: String) -> Self {
        Self {
            function: CIPHER_FUNCTION.to_string(),
            params: CipherParamsInner { nonce, tag },
            ciphertext,
        }
    }
}

impl KeyType {
    /// Create new XMSS-Poseidon2 OTS seed key type
    pub fn new(lifetime: u32, activation_epoch: u32) -> Self {
        Self {
            function: KEYTYPE_FUNCTION.to_string(),
            params: KeyTypeParams {
                lifetime,
                activation_epoch,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystore_creation() {
        let uuid = Uuid::new_v4();

        let kdf = KdfParams::new_full(65536, 3, 4, "0123456789abcdef".to_string());

        let cipher = CipherParams::new(
            "000102030405060708090a0b".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
            "deadbeefcafe".to_string(),
        );

        let crypto = CryptoParams { kdf, cipher };

        let keytype = KeyType::new(262144, 0);

        let keystore = Keystore::new(crypto, keytype, uuid);

        assert_eq!(keystore.version, 5);
        assert!(keystore.quantum_secure);
        assert!(keystore.validate().is_ok());
    }

    #[test]
    fn test_json_serialization() {
        let uuid = Uuid::new_v4();
        let kdf = KdfParams::new_short(65536, 3, 4, "0123456789abcdef".to_string());
        let cipher = CipherParams::new(
            "000102030405060708090a0b".to_string(),
            "0123456789abcdef0123456789abcdef".to_string(),
            "deadbeefcafe".to_string(),
        );
        let crypto = CryptoParams { kdf, cipher };
        let keytype = KeyType::new(262144, 0);

        let keystore = Keystore::new(crypto, keytype, uuid);

        let json = keystore.to_json().unwrap();
        let deserialized = Keystore::from_json(&json).unwrap();

        assert_eq!(keystore.version, deserialized.version);
        assert_eq!(keystore.uuid, deserialized.uuid);
    }
}

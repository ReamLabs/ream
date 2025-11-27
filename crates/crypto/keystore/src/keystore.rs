use std::fs;

use alloy_primitives::B256;
use anyhow::{Result, anyhow, ensure};
use rand;
use ream_bls::{PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{decrypt::aes128_ctr, hex_serde, pbkdf2::pbkdf2, scrypt::scrypt};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct EncryptedKeystore<P = PublicKey, C = CryptoV4> {
    pub crypto: C,
    pub description: String,
    #[serde(rename = "pubkey")]
    pub public_key: P,
    pub path: String,
    pub uuid: String,
    pub version: u64,
}

pub struct Keystore {
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

impl<P, C> EncryptedKeystore<P, C>
where
    P: for<'de> Deserialize<'de> + Serialize,
    C: for<'de> Deserialize<'de> + Serialize,
{
    pub fn load_from_file<Path: AsRef<std::path::Path>>(path: Path) -> Result<Self> {
        Ok(serde_json::from_str(fs::read_to_string(path)?.as_str())?)
    }

    pub fn save_to_file<Path: AsRef<std::path::Path>>(&self, path: Path) -> Result<()> {
        fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
    }
}

impl EncryptedKeystore {
    pub fn validate_password(&self, password: &[u8]) -> anyhow::Result<bool> {
        let derived_key = self.crypto.kdf.params.derive_key(password)?;
        let derived_key_slice = &derived_key[16..32];
        let pre_image = [derived_key_slice, &self.crypto.cipher.message].concat();
        let checksum = Sha256::digest(&pre_image);
        let valid_password = checksum.as_slice() == self.crypto.checksum.message.as_slice();
        Ok(valid_password)
    }

    pub fn decrypt(&self, password: &[u8]) -> anyhow::Result<Keystore> {
        let derived_key = self.crypto.kdf.params.derive_key(password)?;
        let derived_key_slice = &derived_key[16..32];
        let pre_image = [derived_key_slice, &self.crypto.cipher.message].concat();
        let checksum = Sha256::digest(&pre_image);
        ensure!(
            checksum.as_slice() == self.crypto.checksum.message.as_slice(),
            "Password provided is invalid!"
        );

        let mut private_key = PrivateKey {
            inner: B256::from_slice(self.crypto.cipher.message.as_slice()),
        };
        match &self.crypto.cipher.params {
            CipherParams::Aes128Ctr { iv } => {
                let key_param: [u8; 16] = derived_key[0..16].try_into().map_err(|err| {
                    anyhow!("Failed to convert derived key into 16 byte array: {err:?}")
                })?;
                let iv_param: &[u8; 16] = iv.as_slice().try_into().map_err(|err| {
                    anyhow!("Failed to convert derived key into 16 byte array: {err:?}")
                })?;
                aes128_ctr(private_key.inner.as_mut_slice(), key_param, iv_param);
            }
            CipherParams::Aes256Gcm { .. } => todo!(),
        };
        Ok(Keystore {
            public_key: self.public_key.clone(),
            private_key,
        })
    }
}

impl EncryptedKeystore<PublicKey, CryptoV5> {
    /// Create a new keystore from seed phrase and key parameters
    pub fn from_seed_phrase(
        seed_phrase: &str,
        lifetime: u32,
        activation_epoch: u32,
        description: Option<String>,
        path: Option<String>,
    ) -> Self {
        EncryptedKeystore {
            crypto: CryptoV5 {
                kdf: FunctionBlock {
                    params: KdfParams::Argon2Id {
                        m: 65536,
                        t: 4,
                        p: 2,
                        salt: rand::random::<[u8; 32]>().to_vec(),
                    },
                    message: vec![], // Empty message
                },
                cipher: FunctionBlock {
                    params: CipherParams::Aes256Gcm {
                        iv: rand::random::<[u8; 12]>().to_vec(),
                        tag: rand::random::<[u8; 16]>().to_vec(),
                    },
                    // TODO: actually encrypt the seed phrase
                    message: seed_phrase.as_bytes().to_vec(),
                },
                keytype: FunctionBlock {
                    params: KeyTypeParams::XmssPoseidon2OtsSeed {
                        lifetime,
                        activation_epoch,
                    },
                    message: vec![],
                },
            },
            description: description.unwrap_or_default(),
            // TODO: derive the public key from the seed phrase
            public_key: PublicKey::default(),
            path: path.unwrap_or_default(),
            uuid: Uuid::new_v4().to_string(),
            version: 5,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CryptoV4 {
    pub kdf: FunctionBlock<KdfParams>,
    pub checksum: FunctionBlock<ChecksumParams>,
    pub cipher: FunctionBlock<CipherParams>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CryptoV5 {
    pub kdf: FunctionBlock<KdfParams>,
    pub cipher: FunctionBlock<CipherParams>,
    pub keytype: FunctionBlock<KeyTypeParams>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct FunctionBlock<ParamType> {
    #[serde(flatten)]
    pub params: ParamType,
    #[serde(with = "hex_serde")]
    pub message: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "function", content = "params", rename_all = "lowercase")]
pub enum KdfParams {
    Pbkdf2 {
        c: u64,
        dklen: u64,
        prf: Prf,
        #[serde(with = "hex_serde")]
        salt: Vec<u8>,
    },
    Scrypt {
        dklen: u64,
        n: u64,
        p: u64,
        r: u64,
        #[serde(with = "hex_serde")]
        salt: Vec<u8>,
    },
    Argon2Id {
        m: u32,
        t: u32,
        p: u32,
        #[serde(with = "hex_serde")]
        salt: Vec<u8>,
    },
}

impl KdfParams {
    pub fn derive_key(&self, password: &[u8]) -> anyhow::Result<Vec<u8>> {
        match self {
            KdfParams::Pbkdf2 {
                c,
                dklen,
                prf: _,
                salt,
            } => pbkdf2(password, salt, *c, *dklen),
            KdfParams::Scrypt {
                n,
                p,
                r,
                dklen,
                salt,
            } => scrypt(password, salt, *n, *p, *r, *dklen),
            KdfParams::Argon2Id { .. } => todo!(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Prf {
    HmacSha256,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "function", content = "params")]
pub enum CipherParams {
    #[serde(rename = "aes-128-ctr")]
    Aes128Ctr {
        #[serde(with = "hex_serde")]
        iv: Vec<u8>,
    },
    #[serde(rename = "aes-256-gcm")]
    Aes256Gcm {
        #[serde(with = "hex_serde")]
        iv: Vec<u8>,
        #[serde(with = "hex_serde")]
        tag: Vec<u8>,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "function", content = "params", rename_all = "lowercase")]
pub enum ChecksumParams {
    Sha256 {},
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "function", content = "params", rename_all = "kebab-case")]
pub enum KeyTypeParams {
    XmssPoseidon2OtsSeed {
        lifetime: u32,
        activation_epoch: u32,
    },
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;
    use serde_json;
    use ssz_types::FixedVector;

    use super::*;

    #[test]
    fn test_serialization() {
        let keystore = EncryptedKeystore {
            crypto: CryptoV4 {
                kdf: FunctionBlock {
                    params: KdfParams::Scrypt {
                        dklen: 32,
                        n: 262144,
                        p: 1,
                        r: 8,
                        salt: vec![0x12, 0x34, 0x56, 0x78],
                    },
                    message: vec![0x90, 0xAB, 0xCD, 0xEF],
                },
                checksum: FunctionBlock {
                    params: ChecksumParams::Sha256 {},
                    message: vec![0x01, 0x02, 0x03, 0x04],
                },
                cipher: FunctionBlock {
                    params: CipherParams::Aes128Ctr {
                        iv: vec![0xAA, 0xBB, 0xCC, 0xDD],
                    },
                    message: vec![0x11, 0x22, 0x33, 0x44],
                },
            },
            description: "Test Keystore".to_string(),
            public_key: PublicKey {
                inner: FixedVector::try_from(vec![0x12; 48]).unwrap(),
            },
            path: "m/44'/60'/0'/0/0".to_string(),
            uuid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            version: 4,
        };

        let keystore_as_string = r#"{"crypto":{"kdf":{"function":"scrypt","params":{"dklen":32,"n":262144,"p":1,"r":8,"salt":"12345678"},"message":"90abcdef"},"checksum":{"function":"sha256","params":{},"message":"01020304"},"cipher":{"function":"aes-128-ctr","params":{"iv":"aabbccdd"},"message":"11223344"}},"description":"Test Keystore","pubkey":"0x121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212","path":"m/44'/60'/0'/0/0","uuid":"123e4567-e89b-12d3-a456-426614174000","version":4}"#;

        let serialized = serde_json::to_string(&keystore).expect("Failed to serialize keystore");
        assert_eq!(serialized, keystore_as_string);
    }

    #[test]
    fn test_serialization_v5() {
        let keystore = EncryptedKeystore {
            crypto: CryptoV5 {
                kdf: FunctionBlock {
                    params: KdfParams::Argon2Id {
                        m: 65536,
                        t: 4,
                        p: 2,
                        salt: vec![0x12, 0x34, 0x56, 0x78],
                    },
                    message: vec![0x90, 0xAB, 0xCD, 0xEF],
                },
                cipher: FunctionBlock {
                    params: CipherParams::Aes256Gcm {
                        iv: vec![0xAA, 0xBB, 0xCC, 0xDD],
                        tag: vec![0xDE, 0xAD, 0xBE, 0xEF],
                    },
                    message: vec![0x11, 0x22, 0x33, 0x44],
                },
                keytype: FunctionBlock {
                    params: KeyTypeParams::XmssPoseidon2OtsSeed {
                        lifetime: 100000,
                        activation_epoch: 10,
                    },
                    message: vec![],
                },
            },
            description: "".to_string(),
            public_key: PublicKey {
                inner: FixedVector::try_from(vec![0x12; 48]).unwrap(),
            },
            path: "".to_string(),
            uuid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            version: 5,
        };

        let keystore_as_string = r#"{"crypto":{"kdf":{"function":"argon2id","params":{"m":65536,"t":4,"p":2,"salt":"12345678"},"message":"90abcdef"},"cipher":{"function":"aes-256-gcm","params":{"iv":"aabbccdd","tag":"deadbeef"},"message":"11223344"},"keytype":{"function":"xmss-poseidon2-ots-seed","params":{"lifetime":100000,"activation_epoch":10},"message":""}},"description":"","pubkey":"0x121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212121212","path":"","uuid":"123e4567-e89b-12d3-a456-426614174000","version":5}"#;

        let serialized = serde_json::to_string(&keystore).expect("Failed to serialize keystore");
        assert_eq!(serialized, keystore_as_string);
    }

    #[test]
    fn test_deserialization() {
        // The keystore password here is 'password123' for future tests
        let keystore_result = EncryptedKeystore::load_from_file("./assets/ScryptKeystore.json");
        let keystore_deserialized = keystore_result.expect("Failed to deserialize keystore");

        let keystore = EncryptedKeystore {
            crypto: CryptoV4 {
                kdf: FunctionBlock {
                    params: KdfParams::Scrypt {
                        dklen: 32,
                        n: 262144,
                        p: 1,
                        r: 8,
                        salt: hex::decode("a8ba3c3981ec95d49c776f4959720dc04e7ac39a4d8aa26bccf419cb241efd6a")
                            .expect("Failed to decode salt"),
                    },
                    message: vec![], // Empty message
                },
                checksum: FunctionBlock {
                    params: ChecksumParams::Sha256 {},
                    message: hex::decode("7c6392e1b675ea50451ff356206ffe01be7b938eab3b7e2821fcfc0542d90032")
                        .expect("Failed to decode checksum message"),
                },
                cipher: FunctionBlock {
                    params: CipherParams::Aes128Ctr {
                        iv: hex::decode("180742384a64fedc51147799da529dd0").expect("Failed to decode IV"),
                    },
                    message: hex::decode("ae3a00597d61d570b767704edb925e2fe2dd474ea1145c62ac04a2484a322e3d")
                        .expect("Failed to decode cipher message"),
                },
            },
            description: "".to_string(),
            public_key: PublicKey {
                inner: FixedVector::try_from(
                    hex::decode(
                        "b69dfa082ca75d4e50ed4da8fa07d550ba9ec4019815409f42a98b79861d7ad96633a2476594b94c8a6e3048e1b2623e",
                    )
                    .expect("Failed to decode public_key"),
                ).unwrap(),
            },
            path: "m/12381/3600/0/0/0".to_string(),
            uuid: "8f6774f8-3b29-448f-b407-499fb1e98a20".to_string(),
            version: 4,
        };

        assert_eq!(keystore_deserialized, keystore);
    }

    #[test]
    fn test_deserialization_v5() {
        let keystore_result = EncryptedKeystore::<PublicKey, CryptoV5>::load_from_file(
            "./assets/PostQuantumTestKeystore.json",
        );
        let keystore_deserialized = keystore_result.expect("Failed to deserialize keystore");

        let keystore = EncryptedKeystore {
            crypto: CryptoV5 {
                kdf: FunctionBlock {
                    params: KdfParams::Argon2Id {
                        m: 65536,
                        t: 4,
                        p: 2,
                        salt: hex::decode("0a1b2c3d4e5f60718293a4b5c6d7e8f9")
                            .expect("Failed to decode salt"),
                    },
                    message: vec![], // Empty message
                },
                cipher: FunctionBlock {
                    params: CipherParams::Aes256Gcm {
                        iv: hex::decode("cafebabefacedbaddecaf888").expect("Failed to decode IV"),
                        tag: hex::decode("feedfacedeadbeefcafe0000").expect("Failed to decode tag"),
                    },
                    message: hex::decode(
                        "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899",
                    )
                    .expect("Failed to decode cipher message"),
                },
                keytype: FunctionBlock {
                    params: KeyTypeParams::XmssPoseidon2OtsSeed {
                        lifetime: 32,
                        activation_epoch: 28999934,
                    },
                    message: vec![], // Empty message
                },
            },
            description: "".to_string(),
            public_key: PublicKey {
                inner: FixedVector::try_from(
                    hex::decode(
                        "b69dfa082ca75d4e50ed4da8fa07d550ba9ec4019815409f42a98b79861d7ad96633a2476594b94c8a6e3048e1b2623e",
                    )
                    .expect("Failed to decode public_key"),
                ).unwrap(),
            },
            path: "".to_string(),
            uuid: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            version: 5,
        };

        assert_eq!(keystore_deserialized, keystore);
    }

    #[test]
    fn password_validation_pbkdf2() {
        let keystore =
            EncryptedKeystore::load_from_file("./assets/Pbkdf2TestKeystore.json").unwrap();
        let password = hex!("7465737470617373776f7264f09f9491");

        assert!(keystore.validate_password(&password).unwrap());
    }

    #[test]
    fn password_validation_scrypt() {
        let keystore = EncryptedKeystore::load_from_file("./assets/ScryptKeystore.json").unwrap();
        let password = b"password123";

        assert!(keystore.validate_password(password).unwrap());
    }

    #[test]
    fn password_validation_pbkdf2_invalid() {
        let keystore =
            EncryptedKeystore::load_from_file("./assets/Pbkdf2TestKeystore.json").unwrap();
        let password = b"password123";

        assert!(!keystore.validate_password(password).unwrap());
    }

    #[test]
    fn decrypt_pbkdf2() {
        let keystore =
            EncryptedKeystore::load_from_file("./assets/Pbkdf2TestKeystore.json").unwrap();
        let password = hex!("7465737470617373776f7264f09f9491");

        let private_key = hex!("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
        assert_eq!(
            keystore
                .decrypt(&password)
                .unwrap()
                .private_key
                .inner
                .as_slice(),
            private_key
        );
    }

    #[test]
    fn decrypt_scrypt() {
        let keystore =
            EncryptedKeystore::load_from_file("./assets/ScryptDecryptionTest.json").unwrap();
        let password = hex!("7465737470617373776f7264f09f9491");

        let private_key = hex!("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
        assert_eq!(
            keystore
                .decrypt(&password)
                .unwrap()
                .private_key
                .inner
                .as_slice(),
            private_key
        );
    }
}

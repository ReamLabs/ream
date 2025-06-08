use std::{fs, path::Path};

use alloy_primitives::B256;
use anyhow::{Result, anyhow, ensure};
use ream_bls::{PrivateKey, PubKey as PublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{decrypt::aes128_ctr, hex_serde, pbkdf2::pbkdf2, scrypt::scrypt};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct EncryptedKeystore {
    pub crypto: Crypto,
    pub description: String,
    pub pubkey: PublicKey,
    pub path: String,
    pub uuid: String,
    pub version: u64,
}

pub struct Keystore {
    pub public_key: PublicKey,
    pub private_key: PrivateKey,
}

impl EncryptedKeystore {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(serde_json::from_str(fs::read_to_string(path)?.as_str())?)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
    }

    pub fn validate_password(&self, password: &[u8]) -> anyhow::Result<bool> {
        let derived_key = match &self.crypto.kdf.params {
            KdfParams::Pbkdf2 {
                c,
                dklen,
                prf: _,
                salt,
            } => pbkdf2(password, salt, *c, *dklen)?,
            KdfParams::Scrypt {
                n,
                p,
                r,
                dklen,
                salt,
            } => scrypt(password, salt, *n, *p, *r, *dklen)?,
        };
        let derived_key_slice = &derived_key[16..32];
        let pre_image = [derived_key_slice, &self.crypto.cipher.message].concat();
        let checksum = Sha256::digest(&pre_image);
        let valid_password = checksum.as_slice() == self.crypto.checksum.message.as_slice();
        Ok(valid_password)
    }

    pub fn decrypt(&self, password: &[u8]) -> anyhow::Result<Keystore> {
        let derived_key = match &self.crypto.kdf.params {
            KdfParams::Pbkdf2 {
                c,
                dklen,
                prf: _,
                salt,
            } => pbkdf2(password, salt, *c, *dklen)?,
            KdfParams::Scrypt {
                n,
                p,
                r,
                dklen,
                salt,
            } => scrypt(password, salt, *n, *p, *r, *dklen)?,
        };
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
        };
        Ok(Keystore {
            public_key: self.pubkey.clone(),
            private_key,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Crypto {
    pub kdf: FunctionBlock<KdfParams>,
    pub checksum: FunctionBlock<ChecksumParams>,
    pub cipher: FunctionBlock<CipherParams>,
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
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "function", content = "params", rename_all = "lowercase")]
pub enum ChecksumParams {
    Sha256 {},
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
            crypto: Crypto {
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
            pubkey: PublicKey {
                inner: FixedVector::from(vec![0x12; 48]),
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
    fn test_deserialization() {
        // The keystore password here is 'password123' for future tests
        let keystore_result = EncryptedKeystore::load_from_file("./assets/ScryptKeystore.json");
        match keystore_result {
            Ok(keystore_deserialized) => {
                let keystore = EncryptedKeystore {
                    crypto: Crypto {
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
                    pubkey: PublicKey {
                        inner: FixedVector::from(
                            hex::decode(
                                "b69dfa082ca75d4e50ed4da8fa07d550ba9ec4019815409f42a98b79861d7ad96633a2476594b94c8a6e3048e1b2623e",
                            )
                            .expect("Failed to decode pubkey"),
                        ),
                    },
                    path: "m/12381/3600/0/0/0".to_string(),
                    uuid: "8f6774f8-3b29-448f-b407-499fb1e98a20".to_string(),
                    version: 4,
                };

                assert_eq!(keystore_deserialized, keystore);
            }
            Err(err) => {
                panic!("Could not load the keystore: {err:?}");
            }
        }
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

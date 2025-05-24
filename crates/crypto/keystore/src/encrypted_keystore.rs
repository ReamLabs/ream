use std::{fs, path::Path};

use anyhow::Result;
use ream_bls::PubKey;
use serde::{Deserialize, Serialize};

use crate::hex_serde;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct EncryptedKeystore {
    pub crypto: Crypto,
    pub description: String,
    pub pubkey: PubKey,
    pub path: String,
    pub uuid: String,
    pub version: u64,
}

impl EncryptedKeystore {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(serde_json::from_str(fs::read_to_string(path)?.as_str())?)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        fs::write(path, serde_json::to_string(self)?)?;
        Ok(())
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
        c: u32,
        dklen: u8,
        prf: String,
        #[serde(with = "hex_serde")]
        salt: Vec<u8>,
    },
    Scrypt {
        dklen: u8,
        n: u32,
        p: u32,
        r: u32,
        #[serde(with = "hex_serde")]
        salt: Vec<u8>,
    },
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
            pubkey: PubKey {
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
        let keystore_result = EncryptedKeystore::load_from_file("./assets/TestKeystore.json");
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
                    pubkey: PubKey {
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
}

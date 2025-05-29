use ream_bls::PubKey;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Keystore {
    pub crypto: Crypto,
    pub description: String,
    pub pubkey: PubKey,
    pub path: String,
    pub uuid: String,
    pub version: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Crypto {
    pub kdf: FunctionBlock<KdfParamsType>,
    pub checksum: FunctionBlock<Empty>,
    pub cipher: FunctionBlock<CipherParams>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionBlock<ParamType> {
    pub function: String,
    pub params: ParamType,
    pub message: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CipherParams {
    pub iv: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Empty {}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum KdfParamsType {
    Pbkdf2 {
        c: u32,
        dklen: u8,
        prf: String,
        salt: Vec<u8>,
    },
    Scrypt {
        dklen: u8,
        n: u32,
        p: u32,
        r: u32,
        salt: Vec<u8>,
    },
}

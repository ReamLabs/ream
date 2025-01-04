use alloy_primitives::hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz::{Decode, Encode};
use tree_hash::{merkle_root, Hash256, PackedEncoding, TreeHash, TreeHashType};

#[derive(Debug, PartialEq, Clone)]
pub struct BlsSignature {
    pub signature: [u8; 96],
}

impl Encode for BlsSignature {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.signature);
    }
    fn ssz_bytes_len(&self) -> usize {
        96
    }
    fn ssz_fixed_len() -> usize {
        96
    }
}

impl Decode for BlsSignature {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn ssz_fixed_len() -> usize {
        96
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        let mut signature = [0u8; 96];
        signature.copy_from_slice(bytes);
        Ok(Self { signature })
    }
}

impl Serialize for BlsSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = hex::encode(self.signature);
        serializer.serialize_str(&val)
    }
}

impl<'de> Deserialize<'de> for BlsSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result: String = Deserialize::deserialize(deserializer)?;
        let result = hex::decode(&result).map_err(serde::de::Error::custom)?;
        let mut signature = [0u8; 96];
        signature.copy_from_slice(&result);
        Ok(Self { signature })
    }
}

impl TreeHash for BlsSignature {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from_vec(self.signature.to_vec())
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        merkle_root(&self.signature, 1)
    }
}

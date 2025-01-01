use alloy_primitives::{aliases::B32, B256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ForkData {
    pub current_version: B32,
    pub genesis_validators_root: B256,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_primitives::hex::{self, FromHex};
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use tree_hash::TreeHash;

    use super::*;

    #[rstest::rstest]
    #[case(
        "0xfd3120762d23f23e0b663e6ddb24b89ae01b7f2d925e05c9c11ebee4ec23285d",
        "0x248C6C6EEE828430632DD18C6B608EA98806380FE7711B75ED235551BC95DACFC04C158258EB",
        "0x6c6eee82",
        "0x8430632dd18c6b608ea98806380fe7711b75ed235551bc95dacfc04c158258eb"
    )]
    #[case(
        "0x526ad2a5d5c9706cf7f752f92152d9852fca03d41492bc160ce3c6355815c07d",
        "0x248CDB1C95FF566D6E458FC220D2F345BFC063FE717DD26E0C161F70D7CE0D2B2D838077D7B0",
        "0xdb1c95ff",
        "0x566d6e458fc220d2f345bfc063fe717dd26e0c161f70d7ce0d2b2d838077d7b0"
    )]
    fn test_fork_data(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] current_version: &str,
        #[case] genesis_validators_root: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let current_version = B32::from_hex(current_version).unwrap();
        let genesis_validators_root = B256::from_str(genesis_validators_root).unwrap();

        let fork_data = ForkData {
            current_version,
            genesis_validators_root,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, fork_data.as_ssz_bytes());
        assert_eq!(fork_data, ForkData::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(hash_root, fork_data.tree_hash_root());
    }
}

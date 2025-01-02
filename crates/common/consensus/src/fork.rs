use alloy_primitives::aliases::B32;
use ream_common::primitives::Version;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Fork {
    pub previous_version: Version,
    pub current_version: Version,
    pub epoch: u64,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_primitives::hex;
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use tree_hash::TreeHash;

    use super::*;

    #[rstest::rstest]
    #[case(
        "0xda79dfe4112bcf0da8a8b622b9b0f7a6864ec7e4e041a2dd96e28b74fcefc4fb",
        "0x103CF03A40CD1FAB4E41A4D82B3AE5AECF4A",
        "0xf03a40cd",
        "0x1fab4e41",
        5390719578532468900
    )]
    #[case(
        "0x9a90b7beb0f2b6492ca438d9b1d9a7cd802358083c50c94be49cf765b7dd0fe4",
        "0x103CBB01D5CC919E8125A332A21697FF489C",
        "0xbb01d5cc",
        "0x919e8125",
        11261531892624798371
    )]
    fn test_fork(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] previous_version: &str,
        #[case] current_version: &str,
        #[case] epoch: u64,
    ) {
        use alloy_primitives::B256;

        let hash_root = B256::from_str(hash_root).unwrap();
        let previous_version = B32::from_str(previous_version).unwrap();
        let current_version = B32::from_str(current_version).unwrap();

        let fork = Fork {
            previous_version,
            current_version,
            epoch,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, fork.as_ssz_bytes());
        assert_eq!(fork, Fork::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(hash_root, fork.tree_hash_root());
    }
}

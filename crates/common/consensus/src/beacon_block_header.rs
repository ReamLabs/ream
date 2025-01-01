use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body_root: B256,
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_primitives::{
        hex::{self},
        B256,
    };
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use tree_hash::TreeHash;

    use super::*;

    #[rstest::rstest]
    #[case(
        "0x760ede98c4702d54154156bd04cd10a344f7c77a260f68ef015631176b6f790c",
        "0x70F06FFF4A2A310048B0669B84E4068696810EAB95ADBCBF0F9FBF6D899C8A0D73BD2DC098478DAB7F71579AEC55EE7A6641003A6E522DAAC4ACB3584576AB0599CA8C8466E7028A24814AACCC1FBFA8A90755CF98BC58337AC20AE6CCF63F0AEA619289DB4F1FA249027EA994B59A91611FAF",
        7399493353431780095,
        1045282090912089243,
        "0xab95adbcbf0f9fbf6d899c8a0d73bd2dc098478dab7f71579aec55ee7a664100",
        "0x3a6e522daac4acb3584576ab0599ca8c8466e7028a24814aaccc1fbfa8a90755",
        "0xcf98bc58337ac20ae6ccf63f0aea619289db4f1fa249027ea994b59a91611faf",
    )]
    fn test_beacon_block_header(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] slot: u64,
        #[case] proposer_index: u64,
        #[case] parent_root: &str,
        #[case] state_root: &str,
        #[case] body_root: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let parent_root = B256::from_str(parent_root).unwrap();
        let state_root = B256::from_str(state_root).unwrap();
        let body_root = B256::from_str(body_root).unwrap();

        let beacon_block_header = BeaconBlockHeader {
            slot,
            proposer_index,
            parent_root,
            state_root,
            body_root,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, beacon_block_header.as_ssz_bytes());
        assert_eq!(
            beacon_block_header,
            BeaconBlockHeader::from_ssz_bytes(&ssz).unwrap()
        );
        assert_eq!(hash_root, beacon_block_header.tree_hash_root());
    }
}

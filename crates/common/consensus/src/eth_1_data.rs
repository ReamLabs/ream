use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Eth1Data {
    pub deposit_root: B256,
    pub deposit_count: u64,
    pub block_hash: B256,
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
        "0xca3459995a3633f69098d836387f1d428504cac0ee546aa904c1801c68f0dd9f",
        "0x48F047E7E148B0B703BA99E62959BB3E159163C6ADF6FA5095216464803D26C2B9513EA4E9834F161AC73A2710D0D9C4143DFE3861FD1223B7879FAE41C6027BF2565277C526F49826824B",
        "0xe7e148b0b703ba99e62959bb3e159163c6adf6fa5095216464803d26c2b9513e",
        4235382657690888612,
        "0x2710d0d9c4143dfe3861fd1223b7879fae41c6027bf2565277c526f49826824b",
    )]
    #[case(
        "0x41928cbb12b803c8286f2f5e83d0639aa869a83d46a97060e5e9e28f51ac04b2",
        "0x48F047E43140D8009C5EFE0DAB316170952D353671C4965E9D3D7A7EFC89880A59089ABAE344194EC69657993E65DD31004394269E7D41AD3E83213231CF1BC2A1AE246D128CBB12495FAB",
        "0xe43140d8009c5efe0dab316170952d353671c4965e9d3d7a7efc89880a59089a",
        6311449966540022714,
        "0x993e65dd31004394269e7d41ad3e83213231cf1bc2a1ae246d128cbb12495fab",
    )]
    fn test_eth_1_data(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] deposit_root: &str,
        #[case] deposit_count: u64,
        #[case] block_hash: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let deposit_root = B256::from_str(deposit_root).unwrap();
        let block_hash = B256::from_str(block_hash).unwrap();

        let eth_1_data = Eth1Data {
            deposit_root,
            deposit_count,
            block_hash,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, eth_1_data.as_ssz_bytes());
        assert_eq!(eth_1_data, Eth1Data::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(hash_root, eth_1_data.tree_hash_root());
    }
}

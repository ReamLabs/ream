use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::checkpoint::Checkpoint;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AttestationData {
    pub slot: u64,
    pub index: u64,

    /// LMD GHOST vote
    pub beacon_block_root: B256,

    /// FFG vote
    pub source: Checkpoint,
    pub target: Checkpoint,
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_primitives::hex::{self};
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use tree_hash::TreeHash;

    use super::*;

    #[rstest::rstest]
    #[case(
        "0xdcdf6141d043a2f4038f95f86cb20e730f45bc53c1e4a8dd9b2a2144bd70b641",
        "0x8001F07F3351308C8BCCA938524B4D2187DEA63A985CE9B573F521304FBCCEA645EE0A0D82CFF40EA5308A1934AE5DF17E577E24901E03B210E53C36E0381E22F6A62AE082E1A61FD0616D1CB6DB5962AEA8FE8DC73579D758A6E46794C9BAEC44AE6DA34FA4A6DEEB2D334B7673976749A2D32240D507DC070D1D00F6F98E6F38442370",
        4083019436912562483,
        4226309972294454098,
        "0x985ce9b573f521304fbccea645ee0a0d82cff40ea5308a1934ae5df17e577e24",
        3908250436519534224,
        "0xe0381e22f6a62ae082e1a61fd0616d1cb6db5962aea8fe8dc73579d758a6e467",
        11776260211696388500,
        "0x4fa4a6deeb2d334b7673976749a2d32240d507dc070d1d00f6f98e6f38442370"
    )]
    #[case(
        "0xec0fea4b5d5d2a5eeee793ed1820bc1ab15c95ef8a29523552d7a61d7f97aebe",
        "0x8001F07F32A717B4BAC8423ACEBF4CAEC262C6D2115B3E71CFC3F34AEC017109260AE3F5AD380E40282E19E95CD87F60327A918F637C4219790273674E458DA0119164A9DC986851C1809B8CAD5D5A25B999378AAB65460550681D93EDD8A8E626EA8698C295CDB6A83F715A624A771B17478B8EA7A2D1719AD6A294D329854A277AF7AB",
        4198138506873644850,
        15187935381641019342,
        "0x115b3e71cfc3f34aec017109260ae3f5ad380e40282e19e95cd87f60327a918f",
        7454304527366388835,
        "0x4e458da0119164a9dc986851c1809b8cad5d5a25b999378aab65460550681d93",
        10990729393443756269,
        "0xc295cdb6a83f715a624a771b17478b8ea7a2d1719ad6a294d329854a277af7ab"
    )]
    fn test_attestation_data(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] slot: u64,
        #[case] index: u64,
        #[case] beacon_block_root: &str,
        #[case] source_epoch: u64,
        #[case] source_root: &str,
        #[case] target_epoch: u64,
        #[case] target_root: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let beacon_block_root = B256::from_str(beacon_block_root).unwrap();
        let source_root = B256::from_str(source_root).unwrap();
        let target_root = B256::from_str(target_root).unwrap();

        let attestation_data = AttestationData {
            slot,
            index,
            beacon_block_root,
            source: Checkpoint {
                epoch: source_epoch,
                root: source_root,
            },
            target: Checkpoint {
                epoch: target_epoch,
                root: target_root,
            },
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, attestation_data.as_ssz_bytes());
        assert_eq!(
            attestation_data,
            AttestationData::from_ssz_bytes(&ssz).unwrap()
        );
        assert_eq!(hash_root, attestation_data.tree_hash_root());
    }
}

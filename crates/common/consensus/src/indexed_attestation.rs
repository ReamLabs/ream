use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{typenum, VariableList};
use tree_hash_derive::TreeHash;

use crate::{attestation_data::AttestationData, signature::BlsSignature};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct IndexedAttestation {
    pub attesting_indices: VariableList<u64, typenum::U2048>,
    pub data: AttestationData,
    pub signature: BlsSignature,
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
    use ssz_types::FixedVector;
    use tree_hash::TreeHash;

    use super::*;
    use crate::checkpoint::Checkpoint;

    #[rstest::rstest]
    #[case(
        "0xf784abc7913fd2498ea79e001db6a3517d02a587502054a791298df7cb491fc7",
        "0x9C02F41B01E400000060DE5971F11D6A8C577C2CBB099D58A1456265D510B554C7E5CC2BE6136BE9902A7D49E6CB21C36CD12C942CC77EE649C96C5A1F8E49309D9E16333170F366DEFCC0BB8FBD0A9B84BD13D787BBCF86FE3CC58BD05B2CCF65E96034DB24EE9C4CE4A180658CA6213FCD80BEDD8F76F717270AC57EA996599DD9C8CAC57EB93431AA4623D766ED22BD7DC815C2EF07C31FDAF6C6201EC4F833DA452878A63CDF92E6BD2C86EAF2F10146657042310A3E281E9426637AB018F47484FE810DC44F22012A171B232C6EC52557849C05DDE99482E398870CD773807C2E03B45ADB440965E8F4C0B118755A30B191D46B1F0CEBB664239CFD04D548CD2AA6399B54A76FAFD5EA7E85781880CA48F02008547007DF9603E113C8490F",
        [6518143187414214757, 16936946846621872432, 5248106428062983350, 8045492284984535757, 9230259951051134383, 536020749546178762, 1101631571573839583].to_vec(),
        10117932435667279456,
        11626215103177456727,
        "0x456265d510b554c7e5cc2be6136be9902a7d49e6cb21c36cd12c942cc77ee649",
        11326633937597000905,
        "0x9e16333170f366defcc0bb8fbd0a9b84bd13d787bbcf86fe3cc58bd05b2ccf65",
        5520549085313261801,
        "0xe4a180658ca6213fcd80bedd8f76f717270ac57ea996599dd9c8cac57eb93431",
        "0xaa4623d766ed22bd7dc815c2ef07c31fdaf6c6201ec4f833da452878a63cdf92e6bd2c86eaf2f10146657042310a3e281e9426637ab018f47484fe810dc44f22012a171b232c6ec52557849c05dde99482e398870cd773807c2e03b45adb4409"
    )]
    #[case(
        "0xcee62415fafd92e9f47167712f786455686bfb6ea8e408404efaf15646d348ef",
        "0x9C02F41B01E4000000413C51A451F7904C2E0B009A7F9601A33D578ADE0AD164679C2646898A2EBF11AA4810FBB7DDC36972520C116D2057C2CBA0F3F4C5CFF6859B430985A24176362C32D35285E4F44664061EE9A67D4EBF26A77025D3DA79E16E7EF95D00CCE96364658F6491C6CA4A9583C1CE86461EB9C05111A4E834272E1BE4E6F1F0457E97AFC14373BA94D0445DDCFF72DD2B4EDC5DB62D7ABCD6A8AD279ADA054A4160F4C30804C5E458592F5DB9DFFE068769A7798D7B9822B290DF4A42B51E34343CB45B32BBB8C2689A9BC5643F38891283B692C952E60D2335B84BEA90D32E556A772A7BA5C6692761223F5B7D3CB1048FB3777C48582898DE4B3609F0A273D8DEA0A704D4A1AF6FFD7502AADBE408E887DBCC617BEDD4268197",
        [2477304605288266538, 12938565413729295167, 5466974296721620087, 11591940482064714038, 8502074471670351015, 15818867341059140098, 10917049667682001356].to_vec(),
        5517181473550056513,
        11745834777947671342,
        "0x3d578ade0ad164679c2646898a2ebf11aa4810fbb7ddc36972520c116d2057c2",
        9653131300440350923,
        "0x9b430985a24176362c32d35285e4f44664061ee9a67d4ebf26a77025d3da79e1",
        7199509781277146734,
        "0x64658f6491c6ca4a9583c1ce86461eb9c05111a4e834272e1be4e6f1f0457e97",
        "0xafc14373ba94d0445ddcff72dd2b4edc5db62d7abcd6a8ad279ada054a4160f4c30804c5e458592f5db9dffe068769a7798d7b9822b290df4a42b51e34343cb45b32bbb8c2689a9bc5643f38891283b692c952e60d2335b84bea90d32e556a77"
    )]
    fn test_indexed_attestation(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] attesting_indices: Vec<u64>,
        #[case] slot: u64,
        #[case] index: u64,
        #[case] beacon_block_root: &str,
        #[case] source_epoch: u64,
        #[case] source_root: &str,
        #[case] target_epoch: u64,
        #[case] target_root: &str,
        #[case] signature: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let beacon_block_root = B256::from_str(beacon_block_root).unwrap();
        let source_root = B256::from_str(source_root).unwrap();
        let target_root = B256::from_str(target_root).unwrap();
        let signature = BlsSignature {
            signature: FixedVector::from(hex::decode(signature).unwrap()),
        };

        let data = AttestationData {
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

        let indexed_attestation = IndexedAttestation {
            attesting_indices: attesting_indices.into(),
            data,
            signature,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, indexed_attestation.as_ssz_bytes());
        assert_eq!(
            indexed_attestation,
            IndexedAttestation::from_ssz_bytes(&ssz).unwrap()
        );
        assert_eq!(hash_root, indexed_attestation.tree_hash_root());
    }
}

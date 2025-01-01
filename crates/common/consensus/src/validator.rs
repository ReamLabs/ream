use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::pubkey::PubKey;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Validator {
    pub pubkey: PubKey,

    /// Commitment to pubkey for withdrawals
    pub withdrawal_credentials: B256,

    /// Balance at stake
    pub effective_balance: u64,

    pub slashed: bool,

    /// When criteria for activation were met
    pub activation_eligibility_epoch: u64,

    pub activation_epoch: u64,

    pub exit_epoch: u64,

    /// When validator can withdraw funds
    pub withdrawable_epoch: u64,
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy_primitives::hex::{self};
    use snap::raw::Decoder;
    use ssz::{Decode, Encode};
    use ssz_types::FixedVector;
    use tree_hash::TreeHash;

    use super::*;

    #[rstest::rstest]
    #[case(
        "0xc6d4f3f5506c80c5f0ffce2155e4e43267a3285fc4380e00045a90fb34bce2a5",
        "0x79F07884BBB60A37E0B4ED2F9DAC9C241A6202AB1379B6334B58DF82C85D50FF1214663CBA20572FF62528CC9D465A1735F76882BD9CDF6E1D1823576C85B6B0A332FF9D9247D1D6FBE123C6157D54F76D646D9189C26D0912D48E01F917762145E66ED92A5CAEC0EF083CF082E629DDF38A719938AAB52C657F40A1",
        "0x84bbb60a37e0b4ed2f9dac9c241a6202ab1379b6334b58df82c85d50ff1214663cba20572ff62528cc9d465a1735f768",
        "0x82bd9cdf6e1d1823576c85b6b0a332ff9d9247d1d6fbe123c6157d54f76d646d",
        10291870880153897361,
        true,
        15667713338257053689,
        17310720893528202282,
        11056771340163475074,
        11619427111134407224
    )]
    #[case(
        "0x63d108e26ed0cd50252e7ae7d305578095a62f24758e6850ed1c0520ec0a893b",
        "0x79F0783EA925BC0E5E29B2E6A8332B3C5C6526BC3778E7A9CC002A63B81442B04F2240FCCC66F9932BD9BE29411823EC51BBAED11BEFCA4A8BF127EB0EBFAB45F85A6EC30EBB41A5C367CE1A4A0E9C819B1E0C73C2003044652A1F00463445F8667D4B75070124D0B2CCA8DFF2B840129EFE6A104CAFFC31BE692CFD",
        "0x3ea925bc0e5e29b2e6a8332b3c5c6526bc3778e7a9cc002a63b81442b04f2240fccc66f9932bd9be29411823ec51bbae",
        "0xd11befca4a8bf127eb0ebfab45f85a6ec30ebb41a5c367ce1a4a0e9c819b1e0c",
        2245718707735151219,
        false,
        8451987006896288838,
        16116356334913585415,
        1183037807002695922,
        18243072456174382924
    )]
    fn test_validator(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] pubkey: &str,
        #[case] withdrawal_credentials: &str,
        #[case] effective_balance: u64,
        #[case] slashed: bool,
        #[case] activation_eligibility_epoch: u64,
        #[case] activation_epoch: u64,
        #[case] exit_epoch: u64,
        #[case] withdrawable_epoch: u64,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let pubkey = PubKey {
            inner: FixedVector::from(hex::decode(pubkey).unwrap()),
        };
        let withdrawal_credentials = B256::from_str(withdrawal_credentials).unwrap();

        let validator = Validator {
            pubkey,
            withdrawal_credentials,
            effective_balance,
            slashed,
            activation_eligibility_epoch,
            activation_epoch,
            exit_epoch,
            withdrawable_epoch,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, validator.as_ssz_bytes());
        assert_eq!(validator, Validator::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(hash_root, validator.tree_hash_root());
    }
}

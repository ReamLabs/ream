use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::{pubkey::PubKey, signature::BlsSignature};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct DepositData {
    pub pubkey: PubKey,
    pub withdrawal_credentials: B256,
    pub amount: u64,

    /// BLS aggregate signature
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

    #[rstest::rstest]
    #[case(
        "0xaec638527e5c76f14a95ae55b85f65478fbaf0284b20fe77ae885d90a3bbd764",
        "0xB801F0B775EB7F2510D4E27FEDEC8067BD999DF3960F7B63FA051A9535170EE0439C83203C368B0F31E039E4A684AA48132AFDCD6CDC212C75A2FAAC37F5FF410762246D5DE21FE799410A0A72E53CC8D6EDD62EECC8E11AAABA4814FA7FBAF22023AAFD1B4AADED6072515A7AD810C9AA5582FC15D288D658D045C36137244EEC6264E3912DCBA8BA0008D27D6DDB57F8A3D121B90C9B7A1095454738A010BCB5BCC601EC0CB641D373F14138929C5A71AEF32D4042091E2A00C748",
        "0x75eb7f2510d4e27fedec8067bd999df3960f7b63fa051a9535170ee0439c83203c368b0f31e039e4a684aa48132afdcd",
        "0x6cdc212c75a2faac37f5ff410762246d5de21fe799410a0a72e53cc8d6edd62e",
        1461623318839937260,
        "0xfa7fbaf22023aafd1b4aaded6072515a7ad810c9aa5582fc15d288d658d045c36137244eec6264e3912dcba8ba0008d27d6ddb57f8a3d121b90c9b7a1095454738a010bcb5bcc601ec0cb641d373f14138929c5a71aef32d4042091e2a00c748",
    )]
    fn test_deposit_data(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] pubkey: &str,
        #[case] withdrawal_credentials: &str,
        #[case] amount: u64,
        #[case] signature: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let pubkey = PubKey {
            inner: FixedVector::from(hex::decode(pubkey).unwrap()),
        };
        let withdrawal_credentials = B256::from_str(withdrawal_credentials).unwrap();
        let signature = BlsSignature {
            signature: FixedVector::from(hex::decode(signature).unwrap()),
        };
        let deposit_data = DepositData {
            pubkey,
            withdrawal_credentials,
            amount,
            signature,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(ssz, deposit_data.as_ssz_bytes());
        assert_eq!(deposit_data, DepositData::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(hash_root, deposit_data.tree_hash_root());
    }
}

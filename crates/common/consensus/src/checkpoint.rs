use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Checkpoint {
    pub epoch: u64,
    pub root: B256,
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
        "0x96fa7d1a89fe013fd0b9c9a8ea11d1c520cef38d3203c042270c786d4e4b85c4",
        "0x289CBE845725B3EE8D842170688A9E92595FB353C0A2AD6733431A8066C7ECB48AB3B2AAF9091A1722B1",
        9551552837915739326,
        "0x2170688a9e92595fb353c0a2ad6733431a8066c7ecb48ab3b2aaf9091a1722b1"
    )]
    #[case(
        "0x1f1d6eeec3f6c8bbb80de37ba2d0d29a59daabee61572a2972055a29f47948b9",
        "0x289CD3D58F86ACF010C86CBD10DFDB4C97525B757E570C801495F0A8EC2E46D2326461F394B7EAC842F1",
        14416287030995572179,
        "0x6cbd10dfdb4c97525b757e570c801495f0a8ec2e46d2326461f394b7eac842f1"
    )]
    #[case(
        "0x946f6fcd8102bff2183898e462d91709dfa935316a41b5567c7f0e654119c4cd",
        "0x289C121F8CD55AEF40AE274FD59581C40596A1CED51429F815ED4B6724787CCD8509F26AE4B65AE3548D",
        12556298934517767954,
        "0x274fd59581c40596a1ced51429f815ed4b6724787ccd8509f26ae4b65ae3548d"
    )]
    #[case(
        "0x757231e01298f43464173ba4766328dda44bfade6dbec87b4067dc7414e2609a",
        "0x289CE9CD4F54A1B5CFEA3563CEE4609957057CE04AE3D1F182E6EFED1AD86DB8353C80E520FFCBF9500C",
        16919942029563121129,
        "0x3563cee4609957057ce04ae3d1f182e6efed1ad86db8353c80e520ffcbf9500c"
    )]
    fn test_checkpoint(
        #[case] hash_root: &str,
        #[case] snappy_ssz: &str,
        #[case] checkpoint_epoch: u64,
        #[case] checkpoint_root: &str,
    ) {
        let hash_root = B256::from_str(hash_root).unwrap();
        let checkpoint_root = B256::from_str(checkpoint_root).unwrap();

        let checkpoint = Checkpoint {
            epoch: checkpoint_epoch,
            root: checkpoint_root,
        };

        let mut decoder = Decoder::new();
        let snappy_ssz = hex::decode(snappy_ssz).unwrap();
        let ssz = decoder.decompress_vec(&snappy_ssz).unwrap();

        assert_eq!(checkpoint.as_ssz_bytes(), ssz);
        assert_eq!(checkpoint, Checkpoint::from_ssz_bytes(&ssz).unwrap());
        assert_eq!(checkpoint.tree_hash_root(), hash_root);
    }
}

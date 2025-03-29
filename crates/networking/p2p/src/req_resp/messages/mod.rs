pub mod meta_data;

use meta_data::GetMetaDataV2;
use ssz_derive::{Decode, Encode};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum Messages {
    MetaData(GetMetaDataV2),
}

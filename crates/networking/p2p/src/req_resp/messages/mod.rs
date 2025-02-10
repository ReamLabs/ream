pub mod goodbye;
pub mod meta_data;
pub mod ping;
pub mod status;

use std::sync::Arc;

use goodbye::Goodbye;
use meta_data::GetMetaDataV2;
use ping::Ping;
use ssz_derive::{Decode, Encode};
use status::Status;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum Message {
    MetaData(Arc<GetMetaDataV2>),
    Goodbye(Goodbye),
    Status(Status),
    Ping(Ping),
}

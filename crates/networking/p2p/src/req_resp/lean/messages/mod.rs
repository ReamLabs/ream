pub mod blocks;
pub mod status;

use blocks::BlocksByRootV1Request;
use ream_consensus_lean::block::SignedBlock;
use ssz_derive::{Decode, Encode};
use status::Status;

use super::protocol_id::{ProtocolId, SupportedProtocol};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum RequestMessage {
    Status(Status),
    BlocksByRoot(BlocksByRootV1Request),
}

impl RequestMessage {
    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        match self {
            RequestMessage::Status(_) => vec![ProtocolId::new(SupportedProtocol::StatusV1)],
            RequestMessage::BlocksByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::BlocksByRootV1)]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum ResponseMessage {
    Status(Status),
    BlocksByRoot(SignedBlock),
}

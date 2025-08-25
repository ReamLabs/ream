pub mod blocks;
pub mod status;

use blocks::LeanBlocksByRootV1Request;
use ream_consensus_lean::block::SignedBlock;
use ssz_derive::{Decode, Encode};
use status::LeanStatus;

use super::protocol_id::LeanSupportedProtocol;
use crate::req_resp::protocol_id::{ProtocolId, SupportedProtocol};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum LeanRequestMessage {
    Status(LeanStatus),
    BlocksByRoot(LeanBlocksByRootV1Request),
}

impl LeanRequestMessage {
    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        match self {
            LeanRequestMessage::Status(_) => vec![ProtocolId::new(SupportedProtocol::Lean(
                LeanSupportedProtocol::StatusV1,
            ))],
            LeanRequestMessage::BlocksByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::Lean(
                    LeanSupportedProtocol::BlocksByRootV1,
                ))]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum LeanResponseMessage {
    Status(LeanStatus),
    BlocksByRoot(SignedBlock),
}

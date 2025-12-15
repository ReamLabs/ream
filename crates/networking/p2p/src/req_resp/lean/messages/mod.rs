pub mod blocks;
pub mod status;

use std::sync::Arc;

use ream_consensus_lean::block::SignedBlockWithAttestation;
use ssz_derive::{Decode, Encode};

use super::protocol_id::LeanSupportedProtocol;
use crate::req_resp::{
    lean::messages::{blocks::BlocksByRootV1Request, status::Status},
    protocol_id::{ProtocolId, SupportedProtocol},
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum LeanRequestMessage {
    Status(Status),
    BlocksByRoot(BlocksByRootV1Request),
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
    Status(Status),
    BlocksByRoot(Arc<SignedBlockWithAttestation>),
}

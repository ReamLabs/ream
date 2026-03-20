pub mod blocks;
pub mod status;
use std::sync::Arc;

#[cfg(feature = "devnet4")]
use ream_consensus_lean::block::SignedBlock as ActiveBlock;
#[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
use ream_consensus_lean::block::SignedBlockWithAttestation as ActiveBlock;
use ssz_derive::{Decode, Encode};

use super::protocol_id::LeanSupportedProtocol;
use crate::{
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

    pub fn max_response_chunks(&self) -> u64 {
        match self {
            LeanRequestMessage::Status(_) => 1,
            LeanRequestMessage::BlocksByRoot(req) => req.roots.len() as u64,
        }
    }
}

#[cfg(feature = "devnet4")]
pub type ActiveSignedBlock = ream_consensus_lean::block::SignedBlock;

#[cfg(all(feature = "devnet3", not(feature = "devnet4")))]
pub type ActiveSignedBlock = ream_consensus_lean::block::SignedBlockWithAttestation;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum LeanResponseMessage {
    Status(Status),
    BlocksByRoot(Arc<ActiveBlock>),
}

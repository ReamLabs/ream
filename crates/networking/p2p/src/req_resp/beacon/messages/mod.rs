pub mod blob_sidecars;
pub mod blocks;
pub mod goodbye;
pub mod meta_data;
pub mod ping;
pub mod status;

use std::sync::Arc;

use blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request};
use blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request};
use goodbye::Goodbye;
use meta_data::GetMetaDataV2;
use ping::Ping;
use ream_consensus_beacon::{blob_sidecar::BlobSidecar, electra::beacon_block::SignedBeaconBlock};
use ssz_derive::{Decode, Encode};
use status::Status;

use super::protocol_id::{ProtocolId, SupportedProtocol};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum RequestMessage {
    MetaData(Arc<GetMetaDataV2>),
    Goodbye(Goodbye),
    Status(Status),
    Ping(Ping),
    BeaconBlocksByRange(BeaconBlocksByRangeV2Request),
    BeaconBlocksByRoot(BeaconBlocksByRootV2Request),
    BlobSidecarsByRange(BlobSidecarsByRangeV1Request),
    BlobSidecarsByRoot(BlobSidecarsByRootV1Request),
}

impl RequestMessage {
    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        match self {
            RequestMessage::MetaData(_) => vec![ProtocolId::new(SupportedProtocol::GetMetaDataV2)],
            RequestMessage::Goodbye(_) => vec![ProtocolId::new(SupportedProtocol::GoodbyeV1)],
            RequestMessage::Status(_) => vec![ProtocolId::new(SupportedProtocol::StatusV1)],
            RequestMessage::Ping(_) => vec![ProtocolId::new(SupportedProtocol::PingV1)],
            RequestMessage::BeaconBlocksByRange(_) => {
                vec![ProtocolId::new(SupportedProtocol::BeaconBlocksByRangeV2)]
            }
            RequestMessage::BeaconBlocksByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::BeaconBlocksByRootV2)]
            }
            RequestMessage::BlobSidecarsByRange(_) => {
                vec![ProtocolId::new(SupportedProtocol::BlobSidecarsByRangeV1)]
            }
            RequestMessage::BlobSidecarsByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::BlobSidecarsByRootV1)]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum ResponseMessage {
    MetaData(Arc<GetMetaDataV2>),
    Goodbye(Goodbye),
    Status(Status),
    Ping(Ping),
    BeaconBlocksByRange(SignedBeaconBlock),
    BeaconBlocksByRoot(SignedBeaconBlock),
    BlobSidecarsByRange(BlobSidecar),
    BlobSidecarsByRoot(BlobSidecar),
}

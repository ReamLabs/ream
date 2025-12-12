pub mod blob_sidecars;
pub mod blocks;
pub mod data_column_sidecars;
pub mod goodbye;
pub mod meta_data;
pub mod ping;
pub mod status;

use std::sync::Arc;

use blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request};
use blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request};
use data_column_sidecars::{DataColumnSidecarsByRangeV1Request, DataColumnSidecarsByRootV1Request};
use goodbye::Goodbye;
use meta_data::GetMetaDataV3;
use ping::Ping;
use ream_consensus_beacon::{
    blob_sidecar::BlobSidecar, data_column_sidecar::DataColumnSidecar,
    electra::beacon_block::SignedBeaconBlock,
};
use ssz_derive::{Decode, Encode};
use status::Status;

use super::protocol_id::BeaconSupportedProtocol;
use crate::req_resp::protocol_id::{ProtocolId, SupportedProtocol};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum BeaconRequestMessage {
    MetaData(Arc<GetMetaDataV3>),
    Goodbye(Goodbye),
    Status(Status),
    Ping(Ping),
    BeaconBlocksByRange(BeaconBlocksByRangeV2Request),
    BeaconBlocksByRoot(BeaconBlocksByRootV2Request),
    BlobSidecarsByRange(BlobSidecarsByRangeV1Request),
    BlobSidecarsByRoot(BlobSidecarsByRootV1Request),
    DataColumnSidecarsByRange(DataColumnSidecarsByRangeV1Request),
    DataColumnSidecarsByRoot(DataColumnSidecarsByRootV1Request),
}

impl BeaconRequestMessage {
    pub fn supported_protocols(&self) -> Vec<ProtocolId> {
        match self {
            BeaconRequestMessage::MetaData(_) => vec![
                ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::GetMetaDataV3,
                )),
                ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::GetMetaDataV2,
                )),
            ],
            BeaconRequestMessage::Goodbye(_) => vec![ProtocolId::new(SupportedProtocol::Beacon(
                BeaconSupportedProtocol::GoodbyeV1,
            ))],
            BeaconRequestMessage::Status(_) => vec![
                ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::StatusV2,
                )),
                ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::StatusV1,
                )),
            ],
            BeaconRequestMessage::Ping(_) => vec![ProtocolId::new(SupportedProtocol::Beacon(
                BeaconSupportedProtocol::PingV1,
            ))],
            BeaconRequestMessage::BeaconBlocksByRange(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::BeaconBlocksByRangeV2,
                ))]
            }
            BeaconRequestMessage::BeaconBlocksByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::BeaconBlocksByRootV2,
                ))]
            }
            BeaconRequestMessage::BlobSidecarsByRange(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::BlobSidecarsByRangeV1,
                ))]
            }
            BeaconRequestMessage::BlobSidecarsByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::BlobSidecarsByRootV1,
                ))]
            }
            BeaconRequestMessage::DataColumnSidecarsByRange(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::DataColumnSidecarsByRangeV1,
                ))]
            }
            BeaconRequestMessage::DataColumnSidecarsByRoot(_) => {
                vec![ProtocolId::new(SupportedProtocol::Beacon(
                    BeaconSupportedProtocol::DataColumnSidecarsByRootV1,
                ))]
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[ssz(enum_behaviour = "transparent")]
pub enum BeaconResponseMessage {
    MetaData(Arc<GetMetaDataV3>),
    Goodbye(Goodbye),
    Status(Status),
    Ping(Ping),
    BeaconBlocksByRange(SignedBeaconBlock),
    BeaconBlocksByRoot(SignedBeaconBlock),
    BlobSidecarsByRange(BlobSidecar),
    BlobSidecarsByRoot(BlobSidecar),
    DataColumnSidecarsByRange(DataColumnSidecar),
    DataColumnSidecarsByRoot(DataColumnSidecar),
}

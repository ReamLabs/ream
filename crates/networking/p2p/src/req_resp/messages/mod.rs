pub mod beacon_blocks;
pub mod blob_sidecars;
pub mod goodbye;
pub mod meta_data;
pub mod ping;
pub mod status;

use std::sync::Arc;

use beacon_blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request};
use blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request};
use goodbye::Goodbye;
use meta_data::GetMetaDataV2;
use ping::Ping;
use ream_consensus::{blob_sidecar::BlobSidecar, electra::beacon_block::SignedBeaconBlock};
use ssz_derive::{Decode, Encode};
use status::Status;

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

/// All valid protocol name and version combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeaconSupportedProtocol {
    BeaconBlocksByRangeV2,
    BeaconBlocksByRootV2,
    BlobSidecarsByRangeV1,
    BlobSidecarsByRootV1,
    GetMetaDataV2,
    GoodbyeV1,
    PingV1,
    StatusV1,
}

impl BeaconSupportedProtocol {
    pub fn message_name(&self) -> &str {
        match self {
            BeaconSupportedProtocol::BeaconBlocksByRangeV2 => "beacon_blocks_by_range",
            BeaconSupportedProtocol::BeaconBlocksByRootV2 => "beacon_blocks_by_root",
            BeaconSupportedProtocol::BlobSidecarsByRangeV1 => "blob_sidecars_by_range",
            BeaconSupportedProtocol::BlobSidecarsByRootV1 => "blob_sidecars_by_root",
            BeaconSupportedProtocol::GetMetaDataV2 => "metadata",
            BeaconSupportedProtocol::GoodbyeV1 => "goodbye",
            BeaconSupportedProtocol::PingV1 => "ping",
            BeaconSupportedProtocol::StatusV1 => "status",
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            BeaconSupportedProtocol::BeaconBlocksByRangeV2 => "2",
            BeaconSupportedProtocol::BeaconBlocksByRootV2 => "2",
            BeaconSupportedProtocol::BlobSidecarsByRangeV1 => "1",
            BeaconSupportedProtocol::BlobSidecarsByRootV1 => "1",
            BeaconSupportedProtocol::GetMetaDataV2 => "2",
            BeaconSupportedProtocol::GoodbyeV1 => "1",
            BeaconSupportedProtocol::PingV1 => "1",
            BeaconSupportedProtocol::StatusV1 => "1",
        }
    }

    pub fn has_context_bytes(&self) -> bool {
        match self {
            BeaconSupportedProtocol::GetMetaDataV2 => false,
            BeaconSupportedProtocol::GoodbyeV1 => false,
            BeaconSupportedProtocol::PingV1 => false,
            BeaconSupportedProtocol::StatusV1 => false,
            BeaconSupportedProtocol::BeaconBlocksByRangeV2 => true,
            BeaconSupportedProtocol::BeaconBlocksByRootV2 => true,
            BeaconSupportedProtocol::BlobSidecarsByRangeV1 => true,
            BeaconSupportedProtocol::BlobSidecarsByRootV1 => true,
        }
    }
}

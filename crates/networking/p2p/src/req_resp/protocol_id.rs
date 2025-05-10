const PROTOCOL_PREFIX: &str = "/eth2/beacon_chain/req";

#[derive(Debug, Clone)]
pub struct ProtocolId {
    pub protocol_id: String,
    pub protocol: SupportedProtocol,
}

impl ProtocolId {
    pub fn new(protocol: SupportedProtocol) -> Self {
        // Protocol identification `/ProtocolPrefix/MessageName/SchemaVersion/Encoding`
        let protocol_id = format!(
            "{}/{}/{}/ssz_snappy",
            PROTOCOL_PREFIX,
            protocol.message_name(),
            protocol.schema_version()
        );
        ProtocolId {
            protocol_id,
            protocol,
        }
    }
}

impl AsRef<str> for ProtocolId {
    fn as_ref(&self) -> &str {
        &self.protocol_id
    }
}

/// All valid protocol name and version combinations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedProtocol {
    BeaconBlocksByRangeV2,
    BeaconBlocksByRootV2,
    BlobSidecarsByRangeV1,
    BlobSidecarsByRootV1,
    GetMetaDataV2,
    GoodbyeV1,
    PingV1,
    StatusV1,
}

impl SupportedProtocol {
    pub fn message_name(&self) -> &str {
        match self {
            SupportedProtocol::BeaconBlocksByRangeV2 => "beacon_blocks_by_range",
            SupportedProtocol::BeaconBlocksByRootV2 => "beacon_blocks_by_root",
            SupportedProtocol::BlobSidecarsByRangeV1 => "blob_sidecars_by_range",
            SupportedProtocol::BlobSidecarsByRootV1 => "blob_sidecars_by_root",
            SupportedProtocol::GetMetaDataV2 => "metadata",
            SupportedProtocol::GoodbyeV1 => "goodbye",
            SupportedProtocol::PingV1 => "ping",
            SupportedProtocol::StatusV1 => "status",
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            SupportedProtocol::BeaconBlocksByRangeV2 => "2",
            SupportedProtocol::BeaconBlocksByRootV2 => "2",
            SupportedProtocol::BlobSidecarsByRangeV1 => "1",
            SupportedProtocol::BlobSidecarsByRootV1 => "1",
            SupportedProtocol::GetMetaDataV2 => "2",
            SupportedProtocol::GoodbyeV1 => "1",
            SupportedProtocol::PingV1 => "1",
            SupportedProtocol::StatusV1 => "1",
        }
    }

    pub fn supported_protocols() -> Vec<ProtocolId> {
        vec![
            SupportedProtocol::GetMetaDataV2,
            SupportedProtocol::GoodbyeV1,
            SupportedProtocol::PingV1,
            SupportedProtocol::StatusV1,
            SupportedProtocol::BeaconBlocksByRangeV2,
            SupportedProtocol::BeaconBlocksByRootV2,
            SupportedProtocol::BlobSidecarsByRangeV1,
            SupportedProtocol::BlobSidecarsByRootV1,
        ]
        .into_iter()
        .map(ProtocolId::new)
        .collect()
    }

    pub fn has_context_bytes(&self) -> bool {
        match self {
            SupportedProtocol::GetMetaDataV2 => false,
            SupportedProtocol::GoodbyeV1 => false,
            SupportedProtocol::PingV1 => false,
            SupportedProtocol::StatusV1 => false,
            SupportedProtocol::BeaconBlocksByRangeV2 => true,
            SupportedProtocol::BeaconBlocksByRootV2 => true,
            SupportedProtocol::BlobSidecarsByRangeV1 => true,
            SupportedProtocol::BlobSidecarsByRootV1 => true,
        }
    }
}

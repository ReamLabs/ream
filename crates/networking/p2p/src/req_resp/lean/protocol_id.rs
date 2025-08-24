const PROTOCOL_PREFIX: &str = "/leanconsensus/req";

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
    BlocksByRootV1,
    StatusV1,
}

impl SupportedProtocol {
    pub fn message_name(&self) -> &str {
        match self {
            SupportedProtocol::BlocksByRootV1 => "lean_blocks_by_root",
            SupportedProtocol::StatusV1 => "status",
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            SupportedProtocol::BlocksByRootV1 => "1",
            SupportedProtocol::StatusV1 => "1",
        }
    }

    pub fn supported_protocols() -> Vec<ProtocolId> {
        vec![
            SupportedProtocol::BlocksByRootV1,
            SupportedProtocol::StatusV1,
        ]
        .into_iter()
        .map(ProtocolId::new)
        .collect()
    }

    pub fn has_context_bytes(&self) -> bool {
        match self {
            SupportedProtocol::BlocksByRootV1 => false,
            SupportedProtocol::StatusV1 => false,
        }
    }
}

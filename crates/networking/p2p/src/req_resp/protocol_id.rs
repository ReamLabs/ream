use super::{
    Chain, beacon::protocol_id::BeaconSupportedProtocol, lean::protocol_id::LeanSupportedProtocol,
};

const BEACON_PROTOCOL_PREFIX: &str = "/eth2/beacon_chain/req";
const LEAN_PROTOCOL_PREFIX: &str = "/leanconsensus/req";

#[derive(Debug, Clone)]
pub struct ProtocolId {
    pub protocol_id: String,
    pub protocol: SupportedProtocol,
}

impl ProtocolId {
    pub fn new(protocol: SupportedProtocol) -> Self {
        // Protocol identification `/ProtocolPrefix/MessageName/SchemaVersion/Encoding`
        let protocol_id = match protocol {
            SupportedProtocol::Beacon(beacon_protocol) => {
                format!(
                    "{}/{}/{}/ssz_snappy",
                    BEACON_PROTOCOL_PREFIX,
                    beacon_protocol.message_name(),
                    beacon_protocol.schema_version()
                )
            }
            SupportedProtocol::Lean(lean_protocol) => {
                format!(
                    "{}/{}/{}/ssz_snappy",
                    LEAN_PROTOCOL_PREFIX,
                    lean_protocol.message_name(),
                    lean_protocol.schema_version()
                )
            }
        };
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportedProtocol {
    Beacon(BeaconSupportedProtocol),
    Lean(LeanSupportedProtocol),
}

impl SupportedProtocol {
    pub fn message_name(&self) -> &str {
        match self {
            SupportedProtocol::Beacon(beacon_protocol) => beacon_protocol.message_name(),
            SupportedProtocol::Lean(lean_protocol) => lean_protocol.message_name(),
        }
    }

    pub fn schema_version(&self) -> &str {
        match self {
            SupportedProtocol::Beacon(beacon_protocol) => beacon_protocol.schema_version(),
            SupportedProtocol::Lean(lean_protocol) => lean_protocol.schema_version(),
        }
    }

    pub fn has_context_bytes(&self) -> bool {
        match self {
            SupportedProtocol::Beacon(beacon_protocol) => beacon_protocol.has_context_bytes(),
            SupportedProtocol::Lean(lean_protocol) => lean_protocol.has_context_bytes(),
        }
    }

    pub fn supported_protocols(chain: Chain) -> Vec<ProtocolId> {
        match chain {
            Chain::Beacon => vec![
                BeaconSupportedProtocol::GetMetaDataV2,
                BeaconSupportedProtocol::GoodbyeV1,
                BeaconSupportedProtocol::PingV1,
                BeaconSupportedProtocol::StatusV1,
                BeaconSupportedProtocol::BeaconBlocksByRangeV2,
                BeaconSupportedProtocol::BeaconBlocksByRootV2,
                BeaconSupportedProtocol::BlobSidecarsByRangeV1,
                BeaconSupportedProtocol::BlobSidecarsByRootV1,
            ]
            .into_iter()
            .map(SupportedProtocol::Beacon)
            .map(ProtocolId::new)
            .collect(),
            Chain::Lean => vec![
                LeanSupportedProtocol::BlocksByRootV1,
                LeanSupportedProtocol::StatusV1,
            ]
            .into_iter()
            .map(SupportedProtocol::Lean)
            .map(ProtocolId::new)
            .collect(),
        }
    }
}

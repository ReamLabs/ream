use enr::{CombinedKey, Enr};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub peer_id: String,
    pub enr: String,
    pub p2p_address: String,
    pub discovery_address: String,
    pub metadata: MetaData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaData {
    pub seq_number: String,
    pub attnets: String,
    pub syncnets: String,
}

impl Identity {
    pub fn new(
        peer_id: String,
        enr: Enr<CombinedKey>,
        discovery_port: u16,
        socket_addr: String,
        socket_port: u16,
    ) -> Self {
        Self {
            peer_id: peer_id.clone(),
            enr: format!("{}{}", "enr:-", enr.to_base64()),
            p2p_address: format!(
                "{}{}{}{}/{}",
                "/ip4/", socket_addr, "/tcp/", socket_port, peer_id
            ),
            discovery_address: format!("{}{}/p2p/{}", "/ip4/7.7.7.7/udp/", discovery_port, peer_id),
            metadata: MetaData::mock(),
        }
    }
}

impl MetaData {
    pub fn mock() -> Self {
        Self {
            seq_number: String::from("1"),
            attnets: String::from("0x000000"),
            syncnets: String::from("0x0f"),
        }
    }
}

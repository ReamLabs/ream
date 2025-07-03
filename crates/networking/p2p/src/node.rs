use std::net::IpAddr;

use discv5::Enr;
use libp2p::PeerId;

use crate::req_resp::messages::meta_data::GetMetaDataV2;

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub peer_id: PeerId,
    pub enr: Enr,
    pub socket_address: IpAddr,
    pub socket_port: u16,
    pub discovery_port: u16,
    pub meta_data: GetMetaDataV2,
}

impl NodeInfo {
    pub fn from(
        peer_id: PeerId,
        enr: Enr,
        discovery_port: u16,
        socket_address: IpAddr,
        socket_port: u16,
        meta_data: GetMetaDataV2,
    ) -> Self {
        Self {
            peer_id,
            enr,
            socket_address,
            socket_port,
            discovery_port,
            meta_data,
        }
    }
}

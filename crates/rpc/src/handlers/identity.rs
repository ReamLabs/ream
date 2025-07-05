use std::{net::IpAddr, sync::Arc};

use actix_web::{HttpResponse, Responder, get, web::Data};
use discv5::Enr;
use libp2p::PeerId;
use ream_beacon_api_types::{error::ApiError, responses::DataResponse};
use ream_p2p::{network_state::NetworkState, req_resp::messages::meta_data::GetMetaDataV2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Identity {
    pub peer_id: String,
    pub enr: String,
    pub p2p_address: Vec<String>,
    pub discovery_address: Vec<String>,
    pub metadata: GetMetaDataV2,
}

impl Identity {
    pub fn new(
        peer_id: PeerId,
        enr: Enr,
        discovery_port: u16,
        socket_addr: IpAddr,
        socket_port: u16,
        metadata: GetMetaDataV2,
    ) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            enr: enr.to_base64(),
            p2p_address: vec![format!("/ip4/{socket_addr}/tcp/{socket_port}/{peer_id}")],
            discovery_address: vec![format!("/ip4/7.7.7.7/udp/{discovery_port}/p2p/{peer_id}")],
            metadata,
        }
    }
}

/// Called by `eth/v1/node/identity` to get the Node Identity.
#[get("/node/identity")]
pub async fn get_identity(
    network_state: Data<Arc<NetworkState>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(Identity::new(
        network_state.local_peer_id,
        network_state.local_enr.clone(),
        network_state.discovery_port,
        network_state.socket_address,
        network_state.socket_port,
        network_state.meta_data.read().clone(),
    ))))
}

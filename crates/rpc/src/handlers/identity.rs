use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, web::Data};
use discv5::Enr;
use ream_beacon_api_types::{error::ApiError, responses::DataResponse};
use ream_p2p::{
    network::Network, network_state::NetworkState, req_resp::messages::meta_data::GetMetaDataV2,
};
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
    pub fn new(enr: Enr, metadata: GetMetaDataV2) -> Self {
        let peer_id = Network::peer_id_from_enr(&enr).expect("Unable to convert enr to peer id");
        Self {
            peer_id: peer_id.to_string(),
            enr: enr.to_base64(),
            p2p_address: {
                let mut addresses = Vec::new();

                if let Some(ip4) = enr.ip4() {
                    if let Some(tcp4) = enr.tcp4() {
                        addresses.push(format!("/ip4/{}/tcp/{}/p2p/{}", ip4, tcp4, peer_id));
                    }
                }
                if let Some(ip6) = enr.ip6() {
                    if let Some(tcp6) = enr.tcp6() {
                        addresses.push(format!("/ip6/{}/tcp/{}/p2p/{}", ip6, tcp6, peer_id));
                    }
                }

                addresses
            },
            discovery_address: {
                let mut addresses = Vec::new();

                if let Some(ip4) = enr.ip4() {
                    if let Some(udp4) = enr.udp4() {
                        addresses.push(format!("/ip4/{}/udp/{}/p2p/{}", ip4, udp4, peer_id));
                    }
                }
                if let Some(ip6) = enr.ip6() {
                    if let Some(udp6) = enr.udp6() {
                        addresses.push(format!("/ip6/{}/udp/{}/p2p/{}", ip6, udp6, peer_id));
                    }
                }

                addresses
            },
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
        network_state.local_enr.read().clone(),
        network_state.meta_data.read().clone(),
    ))))
}

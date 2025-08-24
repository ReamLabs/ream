use std::sync::Arc;

use actix_web::{HttpResponse, Responder, get, web::Data};
use discv5::Enr;
use ream_api_types_beacon::{error::ApiError, responses::DataResponse};
use ream_p2p::{
    network::{beacon::network_state::NetworkState, misc::peer_id_from_enr},
    req_resp::beacon::messages::meta_data::GetMetaDataV2,
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
        let peer_id = peer_id_from_enr(&enr).expect("Unable to convert enr to peer id");
        Self {
            peer_id: peer_id.to_string(),
            enr: enr.to_base64(),
            p2p_address: {
                let mut addresses = Vec::new();

                if let Some(ip4) = enr.ip4()
                    && let Some(tcp4) = enr.tcp4()
                {
                    addresses.push(format!("/ip4/{ip4}/tcp/{tcp4}/p2p/{peer_id}"));
                }
                if let Some(ip6) = enr.ip6()
                    && let Some(tcp6) = enr.tcp6()
                {
                    addresses.push(format!("/ip6/{ip6}/tcp/{tcp6}/p2p/{peer_id}"));
                }

                addresses
            },
            discovery_address: {
                let mut addresses = Vec::new();

                if let Some(ip4) = enr.ip4()
                    && let Some(udp4) = enr.udp4()
                {
                    addresses.push(format!("/ip4/{ip4}/udp/{udp4}/p2p/{peer_id}"));
                }
                if let Some(ip6) = enr.ip6()
                    && let Some(udp6) = enr.udp6()
                {
                    addresses.push(format!("/ip6/{ip6}/udp/{udp6}/p2p/{peer_id}"));
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

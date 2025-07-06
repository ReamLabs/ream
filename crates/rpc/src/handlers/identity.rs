use std::sync::Arc;

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
    pub fn new(peer_id: PeerId, enr: Enr, metadata: GetMetaDataV2) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            enr: enr.to_base64(),
            p2p_address: vec![match enr.ip4() {
                Some(ip) => format!(
                    "/ip4/{ip}/tcp/{}/p2p/{}",
                    enr.tcp4().expect("tcp4 not set"),
                    peer_id
                ),
                None => format!(
                    "/ip6/{}/tcp/{}/p2p/{}",
                    enr.tcp6().expect("tcp6 not set"),
                    enr.tcp6().expect("tcp6 not set"),
                    peer_id
                ),
            }],
            discovery_address: vec![match enr.ip4() {
                Some(ip) => format!(
                    "/ip4/{ip}/udp/{}/p2p/{}",
                    enr.udp4().expect("udp4 not set"),
                    peer_id
                ),
                None => format!(
                    "/ip6/{}/udp/{}/p2p/{}",
                    enr.tcp6().expect("tcp6 not set"),
                    enr.udp6().expect("udp6 not set"),
                    peer_id
                ),
            }],
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
        network_state.local_enr.read().clone(),
        network_state.meta_data.read().clone(),
    ))))
}

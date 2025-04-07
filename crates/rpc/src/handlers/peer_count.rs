use ream_node::{peer::PeerCountData, network_channel::NetworkChannel};
use serde::{Serialize};
use tracing::error;
use warp::{
    reject::Rejection,
    reply::{Reply},
};

use crate::types::{errors::ApiError};

// Define the response format according to the Ethereum API spec
#[derive(Serialize)]
struct PeerCountResponse {
    data: PeerCountData,
}

pub async fn get_peer_count(network_channel: NetworkChannel) -> Result<impl Reply, Rejection> {
    let count = match network_channel.get_peer_count().await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to receive peer count: {}", e);
            return Err(warp::reject::custom(ApiError::InternalError));
        }
    };

    let response = PeerCountResponse {
        data: PeerCountData {
            disconnecting: count.disconnecting,
            connected: count.connected,
            disconnected: count.disconnected,
            connecting: count.connecting,
        },
    };

    Ok(warp::reply::json(&response))
}

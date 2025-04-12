use ream_node::{network_channel::NetworkChannel};
use tracing::error;
use warp::{
    http::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use crate::{handlers::Data, types::errors::ApiError};

pub async fn get_peer_count(network_channel: NetworkChannel) -> Result<impl Reply, Rejection> {
    let count = match network_channel.get_peer_count().await {
        Ok(count) => count,
        Err(e) => {
            error!("Failed to receive peer count: {}", e);
            return Err(warp::reject::custom(ApiError::InternalError));
        }
    };

    Ok(with_status(Data::json(count), StatusCode::OK))
}

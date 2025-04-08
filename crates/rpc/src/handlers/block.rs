use alloy_primitives::B256;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use crate::types::{
    errors::ApiError,
    id::ID,
    response::{BeaconResponse, RootResponse},
};

/// Called by `/blocks/<block_id>/root` to get the Tree hash of the Block.
pub async fn get_block_root(block_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let block_root = get_block_root_from_id(block_id, db).await?;
    Ok(with_status(
        BeaconResponse::json(RootResponse { root: block_root }),
        StatusCode::OK,
    ))
}

pub async fn get_block_root_from_id(block_id: ID, db: ReamDB) -> Result<B256, ApiError> {
    let block_root = match block_id {
        ID::Finalized => {
            let finalized_checkpoint = db
                .finalized_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Finalized checkpoint not found"))
                })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db
                .justified_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Justified checkpoint not found"))
                })?;

            Ok(Some(justified_checkpoint.root))
        }
        ID::Head | ID::Genesis => {
            return Err(ApiError::NotFound(format!(
                "This ID type is currently not supported: {block_id:?}"
            )));
        }
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => Ok(Some(root)),
    }
    .map_err(|_| ApiError::InternalError)?
    .ok_or(ApiError::NotFound(format!(
        "Failed to find `block_root` from {block_id:?}"
    )))?;
    Ok(block_root)
}

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use alloy_primitives::B256;
use ream_beacon_api_types::{error::ApiError, responses::DataVersionedResponse};
use ream_light_client::bootstrap::LightClientBootstrap;
use ream_storage::{db::ReamDB, tables::Table};
use tracing::error;

#[get("/beacon/light_client/bootstrap/{block_root}")]
pub async fn get_light_client_bootstrap(
    db: Data<ReamDB>,
    block_root: Path<B256>,
) -> Result<impl Responder, ApiError> {
    let block_root = block_root.into_inner();
    let beacon_block = db
        .beacon_block_provider()
        .get(block_root)
        .map_err(|err| {
            error!("Failed to get block by block_root, error: {err:?}");
            ApiError::InternalError
        })?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Failed to find `beacon block` from {block_root:?}"))
        })?;

    let beacon_state = db
        .beacon_state_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))?;

    let light_client_bootstrap =
        LightClientBootstrap::new(&beacon_state, &beacon_block).map_err(|err| {
            error!("Failed to create light client bootstrap, error: {err:?}");
            ApiError::InternalError
        })?;

    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(light_client_bootstrap)))
}

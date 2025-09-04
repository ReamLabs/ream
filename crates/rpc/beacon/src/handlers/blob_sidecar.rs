use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use actix_web_lab::extract::Query;
use ream_api_types_beacon::{query::BlobSidecarQuery, responses::BeaconVersionedResponse};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_consensus_beacon::blob_sidecar::BlobIdentifier;
use ream_storage::{db::beacon::BeaconDB, tables::table::Table};
use tree_hash::TreeHash;

use crate::handlers::block::get_beacon_block_from_id;

#[get("/beacon/blob_sidecars/{block_id}")]
pub async fn get_blob_sidecars(
    db: Data<BeaconDB>,
    block_id: Path<ID>,
    query: Query<BlobSidecarQuery>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;
    let block_root = beacon_block.message.tree_hash_root();

    let indices = if let Some(indices) = &query.indices {
        let max_index = beacon_block.message.body.blob_kzg_commitments.len() as u64;
        for index in indices {
            if index >= &max_index {
                return Err(ApiError::BadRequest(format!(
                    "Invalid blob index: {index}, max index is {max_index}"
                )));
            }
        }
        indices
    } else {
        &(0..beacon_block.message.body.blob_kzg_commitments.len() as u64).collect()
    };

    let mut blob_sidecars = vec![];

    for index in indices {
        let blob_and_proof = db
            .blobs_and_proofs_provider()
            .get(BlobIdentifier::new(block_root, *index))
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get blob and proof for index: {index}, error: {err:?}"
                ))
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to get blob and proof for index: {index}"
            )))?;
        blob_sidecars.push(
            beacon_block
                .blob_sidecar(blob_and_proof, *index)
                .map_err(|err| {
                    ApiError::InternalError(format!(
                        "Failed to create blob sidecar for index: {index}, error: {err:?}"
                    ))
                })?,
        );
    }

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(blob_sidecars)))
}

use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Json, Path},
};
use ream_consensus::blob_sidecar::BlobIdentifier;
use ream_storage::{db::ReamDB, tables::Table};
use tracing::error;
use tree_hash::TreeHash;

use crate::{
    handlers::block::get_beacon_block_from_id,
    types::{errors::ApiError, id::ID, query::BlobSidecarQuery, response::BeaconVersionedResponse},
};

#[get("/beacon/blob_sidecars/{block_id}")]
pub async fn get_blob_sidecars(
    db: Data<ReamDB>,
    block_id: Path<ID>,
    query: Json<BlobSidecarQuery>,
) -> Result<impl Responder, ApiError> {
    let beacon_block = get_beacon_block_from_id(block_id.into_inner(), &db).await?;
    let block_root = beacon_block.message.tree_hash_root();

    let indices = if let Some(indices) = &query.indices {
        let max_index = beacon_block.message.body.blob_kzg_commitments.len() as u64;
        for index in indices {
            if index >= &max_index {
                return Err(ApiError::BadRequest(format!(
                    "Invalid blob index: {index}, max index is {}",
                    max_index - 1
                )));
            }
        }
        indices.clone()
    } else {
        (0..beacon_block.message.body.blob_kzg_commitments.len() as u64).collect()
    };

    let mut blobs = vec![];
    let mut blob_kzg_proofs = vec![];

    for index in indices {
        let blob_and_proof = db
            .blobs_and_proofs_provider()
            .get(BlobIdentifier::new(block_root, index))
            .map_err(|err| {
                error!("Failed to get blob sidecar, error: {err:?}");
                ApiError::InternalError
            })?
            .ok_or(ApiError::InternalError)?;
        blobs.push(blob_and_proof.blob);
        blob_kzg_proofs.push(blob_and_proof.proof);
    }

    let blob_sidecars = beacon_block.get_blob_sidecars(blobs, blob_kzg_proofs);

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(blob_sidecars)))
}

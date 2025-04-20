use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Json, Path},
};
use ream_consensus::blob_sidecar::BlobIdentifier;
use ream_storage::{db::ReamDB, tables::Table};
use tracing::error;

use crate::{
    handlers::block::{get_beacon_block_from_id, get_block_root_from_id},
    types::{errors::ApiError, id::ID, query::BlobSidecarQuery, response::BeaconVersionedResponse},
};

#[get("/beacon/blob_sidecars/{block_id}")]
pub async fn get_blob_sidecars(
    db: Data<ReamDB>,
    block_id: Path<ID>,
    query: Json<BlobSidecarQuery>,
) -> Result<impl Responder, ApiError> {
    let block_id = block_id.into_inner();
    let block_root = get_block_root_from_id(block_id.clone(), &db).await?;
    let beacon_block = get_beacon_block_from_id(block_id, &db).await?;

    let indices = query
        .indices
        .clone()
        .unwrap_or((0..beacon_block.message.body.blob_kzg_commitments.len() as u64).collect());

    let (blobs, blob_kzg_proofs) = {
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
        (blobs, blob_kzg_proofs)
    };

    let blob_sidecars =
        ream_consensus::blob_sidecar::get_blob_sidecars(beacon_block, blobs, blob_kzg_proofs);

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(blob_sidecars)))
}

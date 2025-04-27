use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Json, Path},
};
use ream_consensus::blob_sidecar::{BlobIdentifier, BlobSidecar};
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
    let signed_block_header = beacon_block.compute_signed_block_header();

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

    let mut blob_sidecars = vec![];

    for index in indices {
        let blob_and_proof = db
            .blobs_and_proofs_provider()
            .get(BlobIdentifier::new(block_root, index))
            .map_err(|err| {
                error!("Failed to get blob and proof for index: {index}, error: {err:?}");
                ApiError::InternalError
            })?
            .ok_or(ApiError::NotFound(format!(
                "Failed to get blob and proof for index: {index}"
            )))?;
        blob_sidecars.push(BlobSidecar {
            index,
            blob: blob_and_proof.blob,
            kzg_commitment: beacon_block.message.body.blob_kzg_commitments[index as usize],
            kzg_proof: blob_and_proof.proof,
            signed_block_header: signed_block_header.clone(),
            kzg_commitment_inclusion_proof: beacon_block
                .message
                .body
                .blob_kzg_commitment_inclusion_proof(index)
                .map_err(|err| {
                    error!("Failed to create inclusion proof for blob at index: {index}, error: {err:?}");
                    ApiError::InternalError
                })?
                .into(),
        });
    }

    Ok(HttpResponse::Ok().json(BeaconVersionedResponse::new(blob_sidecars)))
}

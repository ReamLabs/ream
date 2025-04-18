use std::str::FromStr;

use actix_web::{HttpResponse, Responder, error, get, web};
use alloy_primitives::B256;
use ream_consensus::deneb::beacon_block::SignedBeaconBlock;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};

use crate::types::{
    errors::ApiError,
    id::ID,
    response::{BeaconResponse, RootResponse},
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BlockRewards {
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_index: u64,
    #[serde(with = "serde_utils::quoted_i64")]
    pub total: i64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub attestations: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub sync_aggregate: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_slashings: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub attester_slashings: u64,
}

pub async fn get_block_root_from_id(block_id: ID, db: &ReamDB) -> Result<B256, ApiError> {
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

pub async fn get_beacon_block_from_id(
    block_id: ID,
    db: &ReamDB,
) -> Result<SignedBeaconBlock, ApiError> {
    let block_root = get_block_root_from_id(block_id, db).await?;

    db.beacon_block_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon block` from {block_root:?}"
        )))
}

/// Called by `/genesis` to get the Genesis Config of Beacon Chain.
#[get("/beacon/genesis")]
pub async fn get_genesis(db: web::Data<ReamDB>) -> actix_web::Result<impl Responder> {
    Ok(HttpResponse::Ok())
}

/// Called by `/eth/v2/beacon/blocks/{block_id}/attestations` to get block attestations
#[get("/beacon/blocks/{block_id}/attestations")]
pub async fn get_block_attestations(
    db: web::Data<ReamDB>,
    block_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    let block_id = ID::from_str(&block_id.into_inner()).map_err(error::ErrorBadRequest)?;

    let beacon_block = get_beacon_block_from_id(block_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(beacon_block.message.body.attestations))
}

/// Called by `/blocks/<block_id>/root` to get the Tree hash of the Block.
#[get("/beacon/blocks/{block_id}/root")]
pub async fn get_block_root(
    db: web::Data<ReamDB>,
    block_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    let block_id = ID::from_str(&block_id.into_inner()).map_err(error::ErrorBadRequest)?;

    let block_root = get_block_root_from_id(block_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(BeaconResponse::from(RootResponse { root: block_root })))
}

/// Called by `/beacon/blocks/{block_id}/rewards` to get the block rewards response
#[get("/beacon/blocks/{block_id}/rewards")]
pub async fn get_block_rewards(
    db: web::Data<ReamDB>,
    block_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    let block_id = ID::from_str(&block_id.into_inner()).map_err(error::ErrorBadRequest)?;

    let beacon_block = get_beacon_block_from_id(block_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    let response = BlockRewards {
        proposer_index: beacon_block.message.proposer_index,
        total: 0, // todo: implement the calculate block reward logic
        attestations: beacon_block.message.body.attestations.len() as u64,
        sync_aggregate: beacon_block
            .message
            .body
            .sync_aggregate
            .sync_committee_bits
            .num_set_bits() as u64,
        proposer_slashings: beacon_block.message.body.proposer_slashings.len() as u64,
        attester_slashings: beacon_block.message.body.attester_slashings.len() as u64,
    };

    Ok(HttpResponse::Ok().json(BeaconResponse::from(response)))
}

/// Called by `/blocks/<block_id>` to get the Beacon Block.
#[get("/beacon/blocks/{block_id}")]
pub async fn get_block_from_id(
    db: web::Data<ReamDB>,
    block_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    let block_id = ID::from_str(&block_id.into_inner()).map_err(error::ErrorBadRequest)?;

    let beacon_block = get_beacon_block_from_id(block_id, &db)
        .await
        .map_err(error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(beacon_block))
}

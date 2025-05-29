use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path},
};
use ream_bls::PubKey;
use ream_consensus::{constants::SLOTS_PER_EPOCH, misc::compute_start_slot_at_epoch};
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};

use crate::{
    handlers::state::get_state_from_id,
    types::{errors::ApiError, id::ID, response::DutiesResponse},
};

#[derive(Debug, Deserialize, Serialize)]
pub struct ProposerDuty {
    pub pubkey: PubKey,
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
}

#[get("/validator/duties/proposer/{epoch}")]
pub async fn get_proposer_duties(
    db: Data<ReamDB>,
    epoch: Path<u64>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let state = get_state_from_id(ID::Slot(compute_start_slot_at_epoch(epoch)), &db).await?;
    let dependent_root = state
        .get_block_root_at_slot(compute_start_slot_at_epoch(epoch) - 1)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let start_slot = compute_start_slot_at_epoch(epoch);
    let end_slot = start_slot + SLOTS_PER_EPOCH;
    let mut duties = vec![];
    for slot in start_slot..end_slot {
        let validator_index = state
            .get_beacon_proposer_index(Some(slot))
            .map_err(|err| ApiError::BadRequest(err.to_string()))?;
        let Some(validator) = state.validators.get(validator_index as usize) else {
            return Err(ApiError::ValidatorNotFound(format!("{validator_index}")));
        };
        duties.push(ProposerDuty {
            pubkey: validator.pubkey.clone(),
            validator_index,
            slot,
        });
    }
    Ok(HttpResponse::Ok().json(DutiesResponse::new(dependent_root, duties)))
}

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path},
};
use ream_api_types_beacon::{
    duties::{AttesterDuty, ProposerDuty},
    responses::DutiesResponse,
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_consensus_misc::{constants::beacon::SLOTS_PER_EPOCH, misc::compute_start_slot_at_epoch};
use ream_storage::db::beacon::BeaconDB;

use crate::handlers::state::get_state_from_id;

#[get("/validator/duties/proposer/{epoch}")]
pub async fn get_proposer_duties(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let state = get_state_from_id(ID::Slot(compute_start_slot_at_epoch(epoch)), &db).await?;
    let dependent_root = state
        .get_block_root_at_slot(compute_start_slot_at_epoch(epoch) - 1)
        .map_err(|err| ApiError::BadRequest(format!("Failed to get dependent root {err:?}")))?;

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
            public_key: validator.public_key.clone(),
            validator_index,
            slot,
        });
    }
    Ok(HttpResponse::Ok().json(DutiesResponse::new(dependent_root, duties)))
}

#[post("/validator/duties/attester/{epoch}")]
pub async fn get_attester_duties(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
    validator_indices: Json<Vec<u64>>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let state = get_state_from_id(ID::Slot(compute_start_slot_at_epoch(epoch)), &db).await?;
    let dependent_root = state
        .get_block_root_at_slot(compute_start_slot_at_epoch(epoch) - 1)
        .map_err(|err| ApiError::BadRequest(format!("Failed to get dependent root {err:?}")))?;

    let validator_indices = validator_indices.into_inner();
    let committees_at_slot = state.get_committee_count_per_slot(epoch);
    let mut duties = vec![];

    for validator_index in validator_indices {
        let Some(validator) = state.validators.get(validator_index as usize) else {
            return Err(ApiError::ValidatorNotFound(format!(
                "Validator with index {validator_index} not found in state at epoch {epoch}"
            )));
        };

        if let Some((committee, committee_index, slot)) = state
            .get_committee_assignment(epoch, validator_index)
            .map_err(|err| {
                ApiError::BadRequest(format!(
                    "Failed to get committee assignment for validator {validator_index}: {err}"
                ))
            })?
        {
            let validator_committee_index = committee
                .iter()
                .position(|&index| index == validator_index)
                .ok_or_else(|| {
                    ApiError::BadRequest("Validator not found in assigned committee".to_string())
                })?;

            duties.push(AttesterDuty {
                public_key: validator.public_key.clone(),
                validator_index,
                committee_index,
                committees_at_slot,
                validator_committee_index: validator_committee_index as u64,
                slot,
            });
        }
    }
    Ok(HttpResponse::Ok().json(DutiesResponse::new(dependent_root, duties)))
}

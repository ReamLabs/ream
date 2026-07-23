use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path},
};
use alloy_primitives::B256;
use ream_api_types_beacon::{
    duties::{AttesterDuty, ProposerDuty, SyncCommitteeDuty},
    responses::DutiesResponse,
};
use ream_api_types_common::error::ApiError;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{constants::beacon::SLOTS_PER_EPOCH, misc::compute_start_slot_at_epoch};
use ream_storage::{db::beacon::BeaconDB, tables::table::REDBTable};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
enum ValidatorIndexRequest {
    Number(u64),
    String(String),
}

#[get("/validator/duties/proposer/{epoch}")]
pub async fn get_proposer_duties(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let start_slot = compute_start_slot_at_epoch(epoch);
    let state = get_state_at_or_before_slot(&db, start_slot).await?;
    let dependent_root = if epoch == 0 {
        get_block_root_at_or_before_slot(&db, 0)?
    } else {
        get_block_root_at_or_before_slot(&db, start_slot - 1)?
    };
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
    Ok(HttpResponse::Ok().json(DutiesResponse::new(Some(dependent_root), duties)))
}

#[post("/validator/duties/attester/{epoch}")]
pub async fn get_attester_duties(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
    validator_indices: Json<Vec<ValidatorIndexRequest>>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let start_slot = compute_start_slot_at_epoch(epoch);
    let state = get_state_at_or_before_slot(&db, start_slot).await?;
    let dependent_root = if epoch == 0 {
        get_block_root_at_or_before_slot(&db, 0)?
    } else {
        get_block_root_at_or_before_slot(&db, start_slot - 1)?
    };
    let validator_indices = parse_validator_indices(validator_indices.into_inner())?;
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
    Ok(HttpResponse::Ok().json(DutiesResponse::new(Some(dependent_root), duties)))
}

#[post("/validator/duties/sync/{epoch}")]
pub async fn get_sync_committee_duties(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
    validator_indices: Json<Vec<ValidatorIndexRequest>>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let state = get_state_at_or_before_slot(&db, compute_start_slot_at_epoch(epoch)).await?;
    let validator_indices = parse_validator_indices(validator_indices.into_inner())?;

    let mut duties = vec![];
    for validator_index in validator_indices {
        let Some(validator) = state.validators.get(validator_index as usize) else {
            return Err(ApiError::ValidatorNotFound(format!(
                "Validator with index {validator_index} not found in state at epoch {epoch}"
            )));
        };

        let sync_committee_indices = state
            .get_sync_committee_indices(&state.current_sync_committee)
            .map_err(|err| {
                ApiError::BadRequest(format!("Failed to get sync committee indices {err:?}"))
            })?;

        let validator_sync_committee_indices = sync_committee_indices
            .iter()
            .enumerate()
            .filter_map(|(index, &committee_index)| {
                if validator_index == committee_index as u64 {
                    Some(index as u64)
                } else {
                    None
                }
            })
            .collect();

        duties.push(SyncCommitteeDuty {
            public_key: validator.public_key.clone(),
            validator_index,
            validator_sync_committee_indices,
        });
    }
    Ok(HttpResponse::Ok().json(DutiesResponse::new(None, duties)))
}

fn parse_validator_indices(
    validator_indices: Vec<ValidatorIndexRequest>,
) -> Result<Vec<u64>, ApiError> {
    validator_indices
        .into_iter()
        .map(|index| match index {
            ValidatorIndexRequest::Number(index) => Ok(index),
            ValidatorIndexRequest::String(index) => index.parse::<u64>().map_err(|err| {
                ApiError::BadRequest(format!("Invalid validator index `{index}`: {err}"))
            }),
        })
        .collect()
}

async fn get_state_at_or_before_slot(db: &BeaconDB, slot: u64) -> Result<BeaconState, ApiError> {
    let block_root = get_block_root_at_or_before_slot(db, slot)?;
    let mut state = db
        .state_provider()
        .get(block_root)
        .map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to get beacon state by block root, error: {err:?}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::NotFound(format!("Failed to find beacon state for slot {slot}"))
        })?;
    if state.slot < slot {
        state
            .process_slots(slot)
            .map_err(|err| ApiError::BadRequest(err.to_string()))?;
    }
    Ok(state)
}

fn get_block_root_at_or_before_slot(db: &BeaconDB, slot: u64) -> Result<B256, ApiError> {
    for candidate_slot in (0..=slot).rev() {
        match db
            .slot_index_provider()
            .get(candidate_slot)
            .map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get block root for slot {candidate_slot}, error: {err:?}"
                ))
            })? {
            Some(block_root) => return Ok(block_root),
            None => continue,
        }
    }

    Err(ApiError::NotFound(format!(
        "Failed to find block root at or before slot {slot}"
    )))
}

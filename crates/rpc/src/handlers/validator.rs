use ream_consensus::validator::Validator;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::state::get_state_from_id;
use crate::types::{
    errors::ApiError,
    id::{ID, ValidatorID},
    response::BeaconResponse,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorData {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    balance: u64,
    status: String,
    validator: Validator,
}

impl ValidatorData {
    pub fn new(index: u64, balance: u64, status: String, validator: Validator) -> Self {
        Self {
            index,
            balance,
            status,
            validator,
        }
    }
}

pub async fn get_validator_from_state(
    state_id: ID,
    validator_id: ValidatorID,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    let (index, validator) = {
        match &validator_id {
            ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                Some(validator) => (*i as usize, validator.to_owned()),
                None => {
                    return Err(ApiError::ValidatorNotFound(format!(
                        "Validator not found for index: {i}"
                    )))?;
                }
            },
            ValidatorID::Address(pubkey) => {
                match state
                    .validators
                    .iter()
                    .enumerate()
                    .find(|(_, v)| v.pubkey == *pubkey)
                {
                    Some((i, validator)) => (i, validator.to_owned()),
                    None => {
                        return Err(ApiError::ValidatorNotFound(format!(
                            "Validator not found for pubkey: {pubkey:?}"
                        )))?;
                    }
                }
            }
        }
    };

    let balance = state.balances.get(index).ok_or(ApiError::NotFound(format!(
        "Validator not found for index: {index}"
    )))?;

    let status = validator_status(&validator, &db).await?;

    Ok(with_status(
        BeaconResponse::json(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator,
        )),
        StatusCode::OK,
    ))
}

pub async fn validator_status(validator: &Validator, db: &ReamDB) -> Result<String, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let state = get_state_from_id(ID::Slot(highest_slot), db).await?;

    if validator.exit_epoch < state.get_current_epoch() {
        Ok("offline".to_string())
    } else {
        Ok("active_ongoing".to_string())
    }
}

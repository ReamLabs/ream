use std::collections::HashSet;

use ream_consensus::validator::Validator;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use tracing::info;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::state::get_state_from_id;
use crate::types::{
    errors::ApiError,
    id::{ID, ValidatorID},
    query::{IdQuery, StatusQuery},
    request::ValidatorsPostRequest,
    response::BeaconResponse,
    errors::ApiError, id::{ValidatorID, ID}, query::ValidatorBalanceQuery, response::BeaconResponse
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

#[derive(Debug, Serialize)]
struct ValidatorBalance {
    index: String,
    balance: String,
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

pub async fn get_validators_from_state(
    state_id: ID,
    id_query: IdQuery,
    status_query: StatusQuery,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;
    let mut validators_data = Vec::new();
    let mut validator_indices_to_process = Vec::new();

    // First, collect all the validator indices we need to process
    if let Some(validator_ids) = &id_query.id {
        for validator_id in validator_ids {
            let (index, _) = {
                match validator_id {
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
            validator_indices_to_process.push(index);
        }
    } else {
        validator_indices_to_process = (0..state.validators.len()).collect();
    }

    for index in validator_indices_to_process {
        let validator = &state.validators[index];

        let status = validator_status(validator, &db).await?;

        if status_query.has_status() && !status_query.contains_status(&status) {
            continue;
        }

        let balance = state.balances.get(index).ok_or(ApiError::NotFound(format!(
            "Validator not found for index: {index}"
        )))?;

        validators_data.push(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator.clone(),
        ));
    }

    Ok(with_status(
        BeaconResponse::json(validators_data),
        StatusCode::OK,
    ))
}

pub async fn post_validators_from_state(
    state_id: ID,
    request: ValidatorsPostRequest,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let id_query = IdQuery { id: request.ids };

    let status_query = StatusQuery {
        status: request.status,
    };

    get_validators_from_state(state_id, id_query, status_query, db).await
}
pub async fn get_validator_balances_from_state(
    state_id: ID,
    query: ValidatorBalanceQuery, 
    db: ReamDB,
) -> Result<impl Reply, Rejection> {

    let state = get_state_from_id(state_id, &db).await?;

    let filter: Option<HashSet<String>> = match query.id {
        Some(ref ids) if ids.is_empty() => None,
        Some(ids) => Some(ids.into_iter().collect()),
        None => None,
    };

    //need to decide on the limit
    if let Some(ref ids) = filter {
        if ids.len() > 1000 {
            return Err(ApiError::TooManyValidatorIds("Too many validator IDs in request".to_string()))?;
        }
    }

    let validator_balances: Vec<ValidatorBalance> = state
        .validators
        .iter()
        .enumerate()
        .filter_map(|(i, validator)| {
            if let Some(ref ids) = filter {
                if !ids.contains(&i.to_string()) && !ids.contains(&format!("{:?}", validator.pubkey)) {
                    return None;
                }
            }
            let balance = state.balances.get(i).unwrap_or(&0);
            Some(ValidatorBalance {
                index: i.to_string(),
                balance: balance.to_string(),
            })
        })
        .collect();

    let response = BeaconResponse {
        execution_optimistic: false,
        finalized: false,
        data: validator_balances,
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&response),
        StatusCode::OK,
    ))
}

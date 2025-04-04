use ream_consensus::validator::Validator;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::{BeaconResponse, state::get_state_from_id};
use crate::types::{
    errors::ApiError,
    id::{ID, ValidatorID},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorData {
    index: usize,
    balance: u64,
    status: String,
    validator: Validator,
}

impl ValidatorData {
    pub fn new(index: usize, balance: u64, status: String, validator: Validator) -> Self {
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
    let state = get_state_from_id(state_id, db).await?;

    let (index, validator) = {
        match &validator_id {
            ValidatorID::Index(i) => {
                // (*i as usize, state.validators.get(*i as usize)),

                match state.validators.get(*i as usize) {
                    Some(validator) => (*i as usize, validator.to_owned()),
                    None => {
                        return Err(ApiError::ValidatorNotFound(format!(
                            "Validator not found for index: {:?}",
                            i
                        )))?;
                    }
                }
            }
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
                            "Validator not found for pubkey: {:?}",
                            pubkey
                        )))?;
                    }
                }
            }
        }
    };

    let balance = state
        .balances
        .get(index)
        .expect("Unable to fetch validator balance");

    Ok(with_status(
        BeaconResponse::json(ValidatorData::new(
            index,
            *balance,
            "active_ongoing".to_string(),
            validator,
        )),
        StatusCode::OK,
    ))
}

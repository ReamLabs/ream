use ream_consensus::validator::Validator;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    index: String,
    balance: String,
    status: String,
    validator: Validator,
}

impl ValidatorData {
    pub fn new(index: usize, balance: u64, status: String, validator: Validator) -> Self {
        Self {
            index: index.to_string(),
            balance: balance.to_string(),
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
        match validator_id.clone() {
            ValidatorID::Index(i) => (i as usize, state.validators.get(i as usize)),
            ValidatorID::Address(pub_key) => {
                let (i, v) = state
                    .validators
                    .iter()
                    .enumerate()
                    .find(|(_, v)| v.pubkey == pub_key)
                    .unwrap();
                (i, Some(v))
            }
        }
    };

    if validator.is_some() {
        let balance = state
            .balances
            .get(index)
            .expect("Unable to fetch validator balance");
        let validator_data = json!(ValidatorData::new(
            index,
            *balance,
            "active_ongoing".to_string(),
            validator.unwrap().clone()
        ));

        Ok(with_status(
            BeaconResponse::json(validator_data),
            StatusCode::OK,
        ))
    } else {
        Err(ApiError::ValidatorNotFound(validator_id.to_string()))?
    }
}

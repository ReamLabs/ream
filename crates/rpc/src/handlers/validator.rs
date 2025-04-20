use actix_web::{HttpResponse, Responder, error, get, post, web};
use ream_consensus::validator::Validator;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};

use super::state::get_state_from_id;
use crate::types::{
    id::{ID, ValidatorID},
    query::{IdQuery, StatusQuery},
    request::ValidatorsPostRequest,
    response::BeaconResponse,
};

const MAX_VALIDATOR_COUNT: usize = 100;

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

#[get("/beacon/states/{state_id}/validator/{validator_id}")]
pub async fn get_validator_from_state(
    db: web::Data<ReamDB>,
    param: web::Path<(ID, ValidatorID)>,
    web::Json(validator): web::Json<Validator>,
) -> actix_web::Result<impl Responder> {
    let (state_id, validator_id) = param.into_inner();

    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(error::ErrorInternalServerError)?
        .ok_or(error::ErrorNotFound("Failed to find highest slot"))?;

    let state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let (index, validator) = {
        match &validator_id {
            ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                Some(validator) => (*i as usize, validator.to_owned()),
                None => {
                    return Err(error::ErrorNotFound(format!(
                        "Validator not found for index: {i}"
                    )));
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
                        return Err(error::ErrorNotFound(format!(
                            "Validator not found for pubkey: {pubkey:?}"
                        )))?;
                    }
                }
            }
        }
    };

    let balance = state
        .balances
        .get(index)
        .ok_or(error::ErrorNotFound(format!(
            "Validator not found for index: {index}"
        )))?;

    let status = validator_status(&validator, &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator,
        ))),
    )
}

pub async fn validator_status(validator: &Validator, db: &ReamDB) -> actix_web::Result<String> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(error::ErrorInternalServerError)?
        .ok_or(error::ErrorNotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let state = get_state_from_id(ID::Slot(highest_slot), db).await?;

    if validator.exit_epoch < state.get_current_epoch() {
        Ok("offline".to_string())
    } else {
        Ok("active_ongoing".to_string())
    }
}

#[get("/beacon/states/{state_id}/validators")]
pub async fn get_validators_from_state(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
    web::Json(id_query): web::Json<IdQuery>,
    web::Json(status_query): web::Json<StatusQuery>,
) -> actix_web::Result<impl Responder> {
    if let Some(validator_ids) = &id_query.id {
        if validator_ids.len() >= MAX_VALIDATOR_COUNT {
            return Err(error::ErrorNotAcceptable(
                "Too many validator IDs in request",
            ));
        }
    }

    let state = get_state_from_id(state_id.into_inner(), &db).await?;
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
                            return Err(error::ErrorNotFound(format!(
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
                                return Err(error::ErrorNotFound(format!(
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

        let balance = state
            .balances
            .get(index)
            .ok_or(error::ErrorNotFound(format!(
                "Validator not found for index: {index}"
            )))?;

        validators_data.push(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator.clone(),
        ));
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[post("/beacon/states/{state_id}/validators")]
pub async fn post_validators_from_state(
    db: web::Data<ReamDB>,
    state_id: web::Path<ID>,
    web::Json(request): web::Json<ValidatorsPostRequest>,
    web::Json(status_query): web::Json<StatusQuery>,
) -> actix_web::Result<impl Responder> {
    let id_query = IdQuery { id: request.ids };

    let status_query = StatusQuery {
        status: request.status,
    };

    let state = get_state_from_id(state_id.into_inner(), &db).await?;
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
                            return Err(error::ErrorNotFound(format!(
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
                                return Err(error::ErrorNotFound(format!(
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

        let balance = state
            .balances
            .get(index)
            .ok_or(error::ErrorNotFound(format!(
                "Validator not found for index: {index}"
            )))?;

        validators_data.push(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator.clone(),
        ));
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

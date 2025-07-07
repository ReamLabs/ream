use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path, Query},
};
use ream_beacon_api_types::{
    error::ApiError,
    id::{ID, ValidatorID},
    query::{IdQuery, StatusQuery},
    request::{SyncCommitteeSubscription, ValidatorsPostRequest},
    responses::BeaconResponse,
    validator::ValidatorStatus,
};
use ream_bls::PublicKey;
use ream_consensus::validator::Validator;
use ream_discv5::subnet::SyncCommitteeSubnets;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

use super::state::get_state_from_id;

const MAX_VALIDATOR_COUNT: usize = 100;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorData {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    balance: u64,
    status: ValidatorStatus,
    validator: Validator,
}

impl ValidatorData {
    pub fn new(index: u64, balance: u64, status: ValidatorStatus, validator: Validator) -> Self {
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
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    balance: u64,
}

fn build_validator_balances(
    validators_with_balances: &[(Validator, u64)],
    validator_ids: Option<&Vec<ValidatorID>>,
) -> Vec<ValidatorBalance> {
    let mut result = Vec::new();

    for (index, (validator, balance)) in validators_with_balances.iter().enumerate() {
        // If specific validator IDs are requested, filter by them
        if let Some(ids) = validator_ids {
            let should_include = ids.iter().any(|id| match id {
                ValidatorID::Index(i) => *i == index as u64,
                ValidatorID::Address(public_key) => validator.public_key == *public_key,
            });

            if !should_include {
                continue;
            }
        }

        result.push(ValidatorBalance {
            index: index as u64,
            balance: *balance,
        });
    }

    result
}

#[get("/beacon/states/{state_id}/validator/{validator_id}")]
pub async fn get_validator_from_state(
    db: Data<ReamDB>,
    param: Path<(ID, ValidatorID)>,
) -> Result<impl Responder, ApiError> {
    let (state_id, validator_id) = param.into_inner();
    let state = get_state_from_id(state_id, &db).await?;

    let (index, validator) = {
        match &validator_id {
            ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                Some(validator) => (*i as usize, validator.to_owned()),
                None => {
                    return Err(ApiError::NotFound(format!(
                        "Validator not found for index: {i}"
                    )));
                }
            },
            ValidatorID::Address(public_key) => {
                match state
                    .validators
                    .iter()
                    .enumerate()
                    .find(|(_, validator)| validator.public_key == *public_key)
                {
                    Some((i, validator)) => (i, validator.to_owned()),
                    None => {
                        return Err(ApiError::NotFound(format!(
                            "Validator not found for public_key: {public_key:?}"
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

    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(ValidatorData::new(
            index as u64,
            *balance,
            status,
            validator,
        ))),
    )
}

pub async fn validator_status(
    validator: &Validator,
    db: &ReamDB,
) -> Result<ValidatorStatus, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let state = get_state_from_id(ID::Slot(highest_slot), db).await?;

    if validator.exit_epoch < state.get_current_epoch() {
        Ok(ValidatorStatus::Offline)
    } else {
        Ok(ValidatorStatus::ActiveOngoing)
    }
}

#[get("/beacon/states/{state_id}/validators")]
pub async fn get_validators_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    id_query: Query<IdQuery>,
    status_query: Query<StatusQuery>,
) -> Result<impl Responder, ApiError> {
    if let Some(validator_ids) = &id_query.id {
        if validator_ids.len() >= MAX_VALIDATOR_COUNT {
            return Err(ApiError::TooManyValidatorsIds);
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
                            return Err(ApiError::NotFound(format!(
                                "Validator not found for index: {i}"
                            )))?;
                        }
                    },
                    ValidatorID::Address(public_key) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, validator)| validator.public_key == *public_key)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
                                    "Validator not found for public_key: {public_key:?}"
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

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[post("/beacon/states/{state_id}/validators")]
pub async fn post_validators_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    request: Json<ValidatorsPostRequest>,
    _status_query: Json<StatusQuery>,
) -> Result<impl Responder, ApiError> {
    let ValidatorsPostRequest { ids, statuses, .. } = request.into_inner();
    let status_query = StatusQuery { status: statuses };

    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    let mut validators_data = Vec::new();
    let mut validator_indices_to_process = Vec::new();

    // First, collect all the validator indices we need to process
    if let Some(validator_ids) = &ids {
        for validator_id in validator_ids {
            let (index, _) = {
                match validator_id {
                    ValidatorID::Index(i) => match state.validators.get(*i as usize) {
                        Some(validator) => (*i as usize, validator.to_owned()),
                        None => {
                            return Err(ApiError::NotFound(format!(
                                "Validator not found for index: {i}"
                            )))?;
                        }
                    },
                    ValidatorID::Address(public_key) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, validator)| validator.public_key == *public_key)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
                                    "Validator not found for public_key: {public_key:?}"
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

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[derive(Debug, Serialize)]
struct ValidatorIdentity {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    public_key: PublicKey,
    #[serde(with = "serde_utils::quoted_u64")]
    activation_epoch: u64,
}

#[post("/beacon/states/{state_id}/validator_identities")]
pub async fn post_validator_identities_from_state(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    validator_ids: Json<Vec<ValidatorID>>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    let validator_ids_set: HashSet<ValidatorID> = validator_ids.into_inner().into_iter().collect();

    let validator_identities: Vec<ValidatorIdentity> = state
        .validators
        .iter()
        .enumerate()
        .filter_map(|(index, validator)| {
            if validator_ids_set.contains(&ValidatorID::Index(index as u64))
                || validator_ids_set.contains(&ValidatorID::Address(validator.public_key.clone()))
            {
                Some(ValidatorIdentity {
                    index: index as u64,
                    public_key: validator.public_key.clone(),
                    activation_epoch: validator.activation_epoch,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validator_identities)))
}

#[get("/beacon/states/{state_id}/validator_balances")]
pub async fn get_validator_balances_from_state(
    state_id: Path<ID>,
    query: Query<IdQuery>,
    db: Data<ReamDB>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(build_validator_balances(
            &state
                .validators
                .into_iter()
                .zip(state.balances.into_iter())
                .collect::<Vec<_>>(),
            query.id.as_ref(),
        ))),
    )
}

#[post("/beacon/states/{state_id}/validator_balances")]
pub async fn post_validator_balances_from_state(
    state_id: Path<ID>,
    body: Json<IdQuery>,
    db: Data<ReamDB>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(build_validator_balances(
            &state
                .validators
                .into_iter()
                .zip(state.balances.into_iter())
                .collect::<Vec<_>>(),
            body.id.as_ref(),
        ))),
    )
}

async fn process_sync_committee_subscriptions(
    subscriptions: &[SyncCommitteeSubscription],
    sync_committee_subscriptions: &Arc<RwLock<HashMap<u8, u64>>>,
    sync_committee_subnets: &Arc<RwLock<SyncCommitteeSubnets>>,
) -> Result<(), ApiError> {
    let mut subscription_map = sync_committee_subscriptions.write().await;
    let mut subnet_manager = sync_committee_subnets.write().await;
    for subscription in subscriptions.iter() {
        // Parse until_epoch
        let until_epoch: u64 = subscription.until_epoch;
        // Parse and validate sync_committee_indices
        for &index in &subscription.sync_committee_indices {
            let subnet_id: u8 = match index {
                id if id < 4 => id as u8,
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid sync_committee_index: {}",
                        index
                    )));
                }
            };
            subscription_map.insert(subnet_id, until_epoch);
            // Enable the subnet in networking
            if let Err(err) = subnet_manager.enable_sync_committee_subnet(subnet_id) {
                return Err(ApiError::InternalError(format!(
                    "Failed to enable subnet: {err}"
                )));
            }
        }
    }
    info!("Sync committee subnet subscriptions processed successfully");
    Ok(())
}

#[post("/validator/sync_committee_subscriptions")]
pub async fn post_sync_committee_subscriptions(
    subscriptions: Json<Vec<SyncCommitteeSubscription>>,
    sync_committee_subscriptions: Data<Arc<RwLock<HashMap<u8, u64>>>>,
    sync_committee_subnets: Data<Arc<RwLock<SyncCommitteeSubnets>>>,
) -> Result<impl Responder, ApiError> {
    process_sync_committee_subscriptions(
        &subscriptions,
        &sync_committee_subscriptions,
        &sync_committee_subnets,
    )
    .await?;
    Ok(HttpResponse::Ok().json("ok"))
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use tokio::sync::RwLock;

    use super::*;

    #[tokio::test]
    async fn test_process_sync_committee_subscriptions_valid() {
        let subscriptions = vec![SyncCommitteeSubscription {
            validator_index: 1,
            sync_committee_indices: vec![0, 1],
            until_epoch: 10,
        }];
        let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));

        let result = process_sync_committee_subscriptions(
            &subscriptions,
            &sync_committee_subscriptions,
            &sync_committee_subnets,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_sync_committee_subscriptions_invalid_index() {
        let subscriptions = vec![SyncCommitteeSubscription {
            validator_index: 1,
            sync_committee_indices: vec![5],
            until_epoch: 10,
        }];
        let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));

        let result = process_sync_committee_subscriptions(
            &subscriptions,
            &sync_committee_subscriptions,
            &sync_committee_subnets,
        )
        .await;
        assert!(result.is_err());
    }
}

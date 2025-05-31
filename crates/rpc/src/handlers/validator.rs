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
    validator::{ValidatorBalance, ValidatorData, ValidatorStatus},
};
use ream_bls::PubKey;
use ream_consensus::validator::Validator;
use ream_discv5::subnet::SyncCommitteeSubnets;
use ream_storage::db::ReamDB;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};

use super::state::get_state_from_id;

const MAX_VALIDATOR_COUNT: usize = 100;

pub type SyncCommitteeSubscriptionMap = Arc<RwLock<HashMap<u8, u64>>>;

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
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    balance: u64,
}

fn build_validator_balances(
    validators: &[(Validator, u64)],
    filter_ids: Option<&Vec<ValidatorID>>,
) -> Vec<ValidatorBalance> {
    // Turn the optional Vec<ValidatorID> into an optional HashSet for O(1) lookups
    let filtered_ids = filter_ids.map(|ids| ids.iter().collect::<HashSet<_>>());

    validators
        .iter()
        .enumerate()
        .filter(|(idx, (validator, _))| match &filtered_ids {
            Some(ids) => {
                ids.contains(&ValidatorID::Index(*idx as u64))
                    || ids.contains(&ValidatorID::Address(validator.pubkey.clone()))
            }
            None => true,
        })
        .map(|(idx, (_, balance))| ValidatorBalance {
            index: idx as u64,
            balance: *balance,
        })
        .collect()
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
            ValidatorID::Address(pubkey) => {
                match state
                    .validators
                    .iter()
                    .enumerate()
                    .find(|(_, v)| v.pubkey == *pubkey)
                {
                    Some((i, validator)) => (i, validator.to_owned()),
                    None => {
                        return Err(ApiError::NotFound(format!(
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
            error!("Failed to get_highest_slot, error: {err:?}");
            ApiError::InternalError
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
                    ValidatorID::Address(pubkey) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, v)| v.pubkey == *pubkey)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
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
                    ValidatorID::Address(pubkey) => {
                        match state
                            .validators
                            .iter()
                            .enumerate()
                            .find(|(_, v)| v.pubkey == *pubkey)
                        {
                            Some((i, validator)) => (i, validator.to_owned()),
                            None => {
                                return Err(ApiError::NotFound(format!(
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

    Ok(HttpResponse::Ok().json(BeaconResponse::new(validators_data)))
}

#[derive(Debug, Serialize)]
struct ValidatorIdentity {
    #[serde(with = "serde_utils::quoted_u64")]
    index: u64,
    pubkey: PubKey,
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
                || validator_ids_set.contains(&ValidatorID::Address(validator.pubkey.clone()))
            {
                Some(ValidatorIdentity {
                    index: index as u64,
                    pubkey: validator.pubkey.clone(),
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
pub async fn process_sync_committee_subscriptions(
    _db: &ReamDB,
    subscriptions: &[SyncCommitteeSubscription],
    sync_committee_subscriptions: &SyncCommitteeSubscriptionMap,
    sync_committee_subnets: &Arc<RwLock<SyncCommitteeSubnets>>,
) -> Result<(), ApiError> {
    let mut map = sync_committee_subscriptions.write().await;
    let mut subnets = sync_committee_subnets.write().await;
    for sub in subscriptions.iter() {
        // Parse validator_index
        let _validator_index: u64 = match sub.validator_index.parse() {
            Ok(idx) => idx,
            Err(_) => {
                return Err(ApiError::BadRequest(format!(
                    "Invalid validator_index: {}",
                    sub.validator_index
                )));
            }
        };
        // Parse until_epoch
        let until_epoch: u64 = match sub.until_epoch.parse() {
            Ok(epoch) => epoch,
            Err(_) => {
                return Err(ApiError::BadRequest(format!(
                    "Invalid until_epoch: {}",
                    sub.until_epoch
                )));
            }
        };
        // Parse and validate sync_committee_indices
        for idx_str in &sub.sync_committee_indices {
            let subnet_id: u8 = match idx_str.parse() {
                Ok(id) if id < 4 => id,
                _ => {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid sync_committee_index: {}",
                        idx_str
                    )));
                }
            };
            map.insert(subnet_id, until_epoch);
            // Enable the subnet in networking
            if let Err(_e) = subnets.enable_sync_committee_subnet(subnet_id) {
                return Err(ApiError::InternalError);
            }
        }
    }
    info!("Sync committee subnet subscriptions processed successfully");
    Ok(())
}

#[post("/validator/sync_committee_subscriptions")]
pub async fn post_sync_committee_subscriptions(
    db: Data<ReamDB>,
    subscriptions: Json<Vec<SyncCommitteeSubscription>>,
    sync_committee_subscriptions: Data<SyncCommitteeSubscriptionMap>,
    sync_committee_subnets: Data<Arc<RwLock<SyncCommitteeSubnets>>>,
) -> Result<impl Responder, ApiError> {
    process_sync_committee_subscriptions(
        &db,
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

    use tempfile::tempdir;
    use tokio::sync::RwLock;

    use super::*;

    #[tokio::test]
    async fn test_process_sync_committee_subscriptions_valid() {
        let temp_dir = tempdir().unwrap();
        let db = ReamDB::new(temp_dir.path().to_path_buf()).unwrap();
        let subscriptions = vec![SyncCommitteeSubscription {
            validator_index: "1".to_string(),
            sync_committee_indices: vec!["0".to_string(), "1".to_string()],
            until_epoch: "10".to_string(),
        }];
        let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));

        let result = process_sync_committee_subscriptions(
            &db,
            &subscriptions,
            &sync_committee_subscriptions,
            &sync_committee_subnets,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_sync_committee_subscriptions_invalid_index() {
        let temp_dir = tempdir().unwrap();
        let db = ReamDB::new(temp_dir.path().to_path_buf()).unwrap();
        let subscriptions = vec![SyncCommitteeSubscription {
            validator_index: "notanumber".to_string(),
            sync_committee_indices: vec!["0".to_string()],
            until_epoch: "10".to_string(),
        }];
        let sync_committee_subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let sync_committee_subnets = Arc::new(RwLock::new(SyncCommitteeSubnets::new()));

        let result = process_sync_committee_subscriptions(
            &db,
            &subscriptions,
            &sync_committee_subscriptions,
            &sync_committee_subnets,
        )
        .await;
        assert!(result.is_err());
    }
}

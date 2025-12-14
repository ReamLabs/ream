use std::{collections::HashSet, hash::Hash, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path, Query},
};
use alloy_primitives::B256;
use hashbrown::HashMap;
use ream_api_types_beacon::{
    committee::BeaconCommitteeSubscription,
    id::ValidatorID,
    query::{AttestationQuery, IdQuery, StatusQuery},
    request::ValidatorsPostRequest,
    responses::{BeaconResponse, DataResponse},
    validator::{ValidatorBalance, ValidatorData, ValidatorStatus},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_bls::{BLSSignature, PublicKey, traits::Verifiable};
use ream_consensus_beacon::{
    beacon_committee_selection::BeaconCommitteeSelection, electra::beacon_state::BeaconState,
    sync_committe_selection::SyncCommitteeSelection,
};
use ream_consensus_misc::{
    attestation_data::AttestationData,
    constants::beacon::{
        DOMAIN_AGGREGATE_AND_PROOF, DOMAIN_BEACON_ATTESTER, DOMAIN_RANDAO, DOMAIN_SYNC_COMMITTEE,
        MAX_COMMITTEES_PER_SLOT, SLOTS_PER_EPOCH,
    },
    misc::{compute_domain, compute_epoch_at_slot, compute_signing_root},
    validator::Validator,
};
use ream_events_beacon::{
    BeaconEvent, contribution_and_proof::SignedContributionAndProof,
    event::sync_committee::ContributionAndProofEvent,
};
use ream_execution_engine::{
    ExecutionEngine,
    rpc_types::forkchoice_update::{ForkchoiceStateV1, PayloadAttributesV3},
};
use ream_fork_choice_beacon::store::Store;
use ream_network_manager::gossipsub::validate::sync_committee_contribution_and_proof::get_sync_subcommittee_pubkeys;
use ream_operation_pool::OperationPool;
use ream_storage::{db::beacon::BeaconDB, tables::field::REDBField};
use ream_validator_beacon::{
    aggregate_and_proof::SignedAggregateAndProof,
    attestation::compute_subnet_for_attestation,
    builder::validator_registration::SignedValidatorRegistrationV1,
    constants::{
        DOMAIN_CONTRIBUTION_AND_PROOF, DOMAIN_SELECTION_PROOF,
        DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF, SYNC_COMMITTEE_SUBNET_COUNT,
    },
    sync_committee::{SyncAggregatorSelectionData, is_sync_committee_aggregator},
};
use serde::Serialize;
use tokio::sync::broadcast;

use super::state::get_state_from_id;

///  For slots in Electra and later, this AttestationData must have a committee_index of 0.
const ELECTRA_COMMITTEE_INDEX: u64 = 0;
const MAX_VALIDATOR_COUNT: usize = 100;

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
                    || ids.contains(&ValidatorID::Address(validator.public_key.clone()))
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
    db: Data<BeaconDB>,
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
                match find_validator_by_public_key(&state, public_key) {
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
    db: &BeaconDB,
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
    let current_epoch = state.get_current_epoch();

    // Check if validator is pending (not yet activated)
    if validator.activation_epoch > current_epoch {
        Ok(ValidatorStatus::Pending)
    }
    // Check if validator has exited
    else if validator.exit_epoch <= current_epoch {
        Ok(ValidatorStatus::Offline)
    }
    // Validator is active
    else {
        Ok(ValidatorStatus::ActiveOngoing)
    }
}

/// Helper function to find validator by public key in state
fn find_validator_by_public_key<'a>(
    state: &'a BeaconState,
    public_key: &PublicKey,
) -> Option<(usize, &'a Validator)> {
    state
        .validators
        .iter()
        .enumerate()
        .find(|(_, v)| v.public_key == *public_key)
}

#[get("/beacon/states/{state_id}/validators")]
pub async fn get_validators_from_state(
    db: Data<BeaconDB>,
    state_id: Path<ID>,
    id_query: Query<IdQuery>,
    status_query: Query<StatusQuery>,
) -> Result<impl Responder, ApiError> {
    if let Some(validator_ids) = &id_query.id
        && validator_ids.len() >= MAX_VALIDATOR_COUNT
    {
        return Err(ApiError::TooManyValidatorsIds);
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
                        match find_validator_by_public_key(&state, public_key) {
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
    db: Data<BeaconDB>,
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
                        match find_validator_by_public_key(&state, public_key) {
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
    db: Data<BeaconDB>,
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
    db: Data<BeaconDB>,
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
    db: Data<BeaconDB>,
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

#[derive(Debug, Serialize)]
pub struct ValidatorLivenessData {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    pub is_live: bool,
}

impl ValidatorLivenessData {
    pub fn new(index: u64, is_live: bool) -> Self {
        Self { index, is_live }
    }
}

#[post("/validator/liveness/{epoch}")]
pub async fn post_validator_liveness(
    db: Data<BeaconDB>,
    epoch: Path<u64>,
    validator_indices: Json<Vec<String>>,
) -> Result<impl Responder, ApiError> {
    let epoch = epoch.into_inner();
    let validator_indices = validator_indices.into_inner();

    let slot = epoch * SLOTS_PER_EPOCH;
    let state = get_state_from_id(ID::Slot(slot), &db).await?;

    let mut liveness_data = Vec::new();

    for validator_index_str in validator_indices {
        let validator_index: u64 = validator_index_str
            .parse()
            .map_err(|err| ApiError::BadRequest(format!("Invalid validator index: {err:?}")))?;
        let index = validator_index as usize;

        match state.validators.get(index) {
            Some(_validator) => {
                let is_live = check_validator_participation(&state, index, epoch)?;
                liveness_data.push(ValidatorLivenessData::new(validator_index, is_live));
            }
            None => continue,
        }
    }

    Ok(HttpResponse::Ok().json(BeaconResponse::new(liveness_data)))
}

fn check_validator_participation(
    state: &BeaconState,
    validator_index: usize,
    epoch: u64,
) -> Result<bool, ApiError> {
    let validator = &state.validators[validator_index];
    if !validator.is_active_validator(epoch) {
        return Ok(false);
    }

    let current_epoch = state.get_current_epoch();

    if epoch == current_epoch {
        if let Some(participation) = state.current_epoch_participation.get(validator_index) {
            Ok(*participation > 0)
        } else {
            Ok(false)
        }
    } else if epoch == current_epoch - 1 {
        if let Some(participation) = state.previous_epoch_participation.get(validator_index) {
            Ok(*participation > 0)
        } else {
            Ok(false)
        }
    } else {
        Ok(validator.is_active_validator(epoch))
    }
}
#[get("/validator/attestation_data")]
pub async fn get_attestation_data(
    db: Data<BeaconDB>,
    opertation_pool: Data<Arc<OperationPool>>,
    query: Query<AttestationQuery>,
) -> Result<impl Responder, ApiError> {
    let store = Store {
        db: db.get_ref().clone(),
        operation_pool: opertation_pool.get_ref().clone(),
    };

    if store.is_syncing().map_err(|err| {
        ApiError::InternalError(format!("Failed to check syncing status, err: {err:?}"))
    })? {
        return Err(ApiError::UnderSyncing);
    }

    let slot = query.slot;

    let current_slot = store
        .get_current_slot()
        .map_err(|err| ApiError::InternalError(format!("Failed to slot_index, error: {err:?}")))?;

    if slot > current_slot + 1 {
        return Err(ApiError::InvalidParameter(format!(
            "Slot {slot:?} is too far ahead of the current slot {current_slot:?}"
        )));
    }

    let beacon_block_root = db
        .slot_index_provider()
        .get_highest_root()
        .map_err(|err| ApiError::InternalError(format!("Failed to slot_index, error: {err:?}")))?
        .ok_or(ApiError::NotFound(
            "Failed to find highest block root".to_string(),
        ))?;

    let source_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
        ApiError::InternalError(format!("Failed to get source checkpoint, error: {err:?}"))
    })?;

    let target_checkpoint = db
        .unrealized_justified_checkpoint_provider()
        .get()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to target checkpoint, error: {err:?}"))
        })?;

    Ok(HttpResponse::Ok().json(DataResponse::new(AttestationData {
        slot,
        index: ELECTRA_COMMITTEE_INDEX,
        beacon_block_root,
        source: source_checkpoint,
        target: target_checkpoint,
    })))
}

/// For the initial stage, this endpoint returns a 501 as DVT support is not planned.
#[post("/validator/sync_committee_selections")]
pub async fn post_sync_committee_selections(
    _selections: Json<SyncCommitteeSelection>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::NotImplemented())
}

/// For the initial stage, this endpoint returns a 501 as DVT support is not planned.
#[post("/validator/beacon_committee_selections")]
pub async fn post_beacon_committee_selections(
    _selections: Json<Vec<BeaconCommitteeSelection>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::NotImplemented())
}

#[post("/validator/aggregate_and_proofs")]
pub async fn post_aggregate_and_proofs_v2(
    db: Data<BeaconDB>,
    aggregates: Json<Vec<SignedAggregateAndProof>>,
) -> Result<impl Responder, ApiError> {
    for signed_aggregate in aggregates.into_inner() {
        let aggregate_and_proof = signed_aggregate.message;
        let attestation = aggregate_and_proof.aggregate.clone();
        let slot = attestation.data.slot;
        let state = get_state_from_id(ID::Slot(slot), &db).await?;

        let aggregator_index = aggregate_and_proof.aggregator_index as usize;

        let aggregator = state
            .validators
            .get(aggregator_index)
            .ok_or_else(|| ApiError::NotFound("Aggregator not found".to_string()))?;

        let committee = state
            .get_beacon_committee(attestation.data.slot, attestation.data.index)
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?;

        if !committee.contains(&(aggregator_index as u64)) {
            return Err(ApiError::BadRequest(
                "Aggregator not part of the committee".to_string(),
            ));
        }

        let aggregator_selection_domain =
            state.get_domain(DOMAIN_SELECTION_PROOF, Some(compute_epoch_at_slot(slot)));
        let aggregator_selection_signing_root =
            compute_signing_root(attestation.data.slot, aggregator_selection_domain);

        if !aggregate_and_proof
            .selection_proof
            .verify(
                &aggregator.public_key,
                aggregator_selection_signing_root.as_ref(),
            )
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Aggregator selection proof is not valid".to_string(),
            ));
        }

        let committee_pub_keys: Vec<&PublicKey> = committee
            .iter()
            .enumerate()
            .filter(|(i, _)| attestation.aggregation_bits.get(*i).unwrap_or(false))
            .map(|(i, _)| &state.validators[committee[i] as usize].public_key)
            .collect();

        if committee_pub_keys.is_empty() {
            return Err(ApiError::BadRequest(
                "No aggregation bits set in the attestation".into(),
            ));
        }

        let aggregate_signature_domain =
            state.get_domain(DOMAIN_BEACON_ATTESTER, Some(attestation.data.target.epoch));
        let aggregate_signature_signing_root =
            compute_signing_root(&attestation.data, aggregate_signature_domain);

        if !attestation
            .signature
            .fast_aggregate_verify(
                committee_pub_keys,
                aggregate_signature_signing_root.as_ref(),
            )
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Aggregated signature verification failed".to_string(),
            ));
        }

        let aggregate_proof_domain = state.get_domain(
            DOMAIN_AGGREGATE_AND_PROOF,
            Some(compute_epoch_at_slot(attestation.data.slot)),
        );
        let aggregate_proof_signing_root =
            compute_signing_root(aggregate_and_proof, aggregate_proof_domain);

        if !signed_aggregate
            .signature
            .verify(
                &aggregator.public_key,
                aggregate_proof_signing_root.as_ref(),
            )
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Aggregate proof verification failed".to_string(),
            ));
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": "success"
    })))
}

#[post("/validator/beacon_committee_subscriptions")]
pub async fn post_beacon_committee_subscriptions(
    db: Data<BeaconDB>,
    subscriptions: Json<Vec<BeaconCommitteeSubscription>>,
) -> Result<impl Responder, ApiError> {
    let mut subnet_to_subscriptions: HashMap<u64, Vec<BeaconCommitteeSubscription>> =
        HashMap::new();
    for sub in subscriptions.into_inner() {
        let state = get_state_from_id(ID::Slot(sub.slot), &db).await?;
        if sub.committees_at_slot > MAX_COMMITTEES_PER_SLOT {
            return Err(ApiError::BadRequest(
                "Committees at a slot should be less than the maximum committees per slot".into(),
            ));
        }
        if sub.committee_index >= sub.committees_at_slot {
            return Err(ApiError::BadRequest(
                "Committee index cannot be more than the committees in a slot".into(),
            ));
        }

        let committee_members = state
            .get_beacon_committee(sub.slot, sub.committee_index)
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?;

        if !committee_members.contains(&sub.validator_index) {
            return Err(ApiError::BadRequest(
                "Validator not part of the committee".to_string(),
            ));
        }

        let subnet_id =
            compute_subnet_for_attestation(sub.committees_at_slot, sub.slot, sub.committee_index);

        subnet_to_subscriptions
            .entry(subnet_id)
            .or_default()
            .push(sub);
    }

    // TODO
    // add support for attestation subnet subscriptions for validators

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": "success"
    })))
}

/// Verify validator registration signature
fn verify_validator_registration_signature(
    signed_registration: &SignedValidatorRegistrationV1,
) -> Result<bool, ApiError> {
    use ream_validator_beacon::builder::DOMAIN_APPLICATION_BUILDER;

    let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);
    let signing_root = compute_signing_root(signed_registration.message.clone(), domain);

    signed_registration
        .signature
        .verify(
            &signed_registration.message.public_key,
            signing_root.as_ref(),
        )
        .map_err(|err| ApiError::InternalError(format!("Signature verification failed: {err:?}")))
}

#[post("/validator/register_validator")]
pub async fn post_register_validator(
    db: Data<BeaconDB>,
    builder_client: Option<
        Data<Arc<ream_validator_beacon::builder::builder_client::BuilderClient>>,
    >,
    registrations: Json<Vec<SignedValidatorRegistrationV1>>,
) -> Result<impl Responder, ApiError> {
    let registrations = registrations.into_inner();

    if registrations.is_empty() {
        return Err(ApiError::BadRequest("Empty request body".to_string()));
    }

    // Get the current state once for all validator status checks
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let state = get_state_from_id(ID::Slot(highest_slot), &db).await?;
    let current_epoch = state.get_current_epoch();

    for registration in registrations {
        // Verify signature
        let signature_valid = verify_validator_registration_signature(&registration)?;
        if !signature_valid {
            continue;
        }

        // Check if validator is active or pending (not exited or unknown)
        let is_valid = if let Some((_index, validator)) =
            find_validator_by_public_key(&state, &registration.message.public_key)
        {
            let is_pending = validator.activation_epoch > current_epoch;
            let is_active =
                validator.activation_epoch <= current_epoch && current_epoch < validator.exit_epoch;
            is_pending || is_active
        } else {
            false
        };

        if !is_valid {
            continue;
        }

        // Forward immediately to builder if available
        if let Some(ref client) = builder_client {
            client
                .register_validator(registration)
                .await
                .map_err(|err| {
                    ApiError::InternalError(format!("Failed to forward to builder: {err}"))
                })?;
        }
    }

    Ok(HttpResponse::Ok().body("Validator registrations have been received."))
}

#[derive(Clone, Debug, Serialize)]
struct ContributionAndProofFailure {
    index: usize,
    message: String,
}

fn validate_signed_contribution_and_proof(
    signed_contribution_and_proof: &SignedContributionAndProof,
    state: &BeaconState,
) -> Result<(), String> {
    let contribution_and_proof = &signed_contribution_and_proof.message;
    let contribution = &contribution_and_proof.contribution;
    let epoch = compute_epoch_at_slot(contribution.slot);

    if contribution.subcommittee_index >= SYNC_COMMITTEE_SUBNET_COUNT {
        return Err("The subcommittee index is out of range".to_string());
    }

    if contribution.aggregation_bits.num_set_bits() == 0 {
        return Err("The contribution has no participants".to_string());
    }

    if !is_sync_committee_aggregator(&contribution_and_proof.selection_proof) {
        return Err("The selection proof is not a valid aggregator".to_string());
    }

    let aggregator_index = usize::try_from(contribution_and_proof.aggregator_index)
        .map_err(|err| format!("Invalid aggregator index: {err:?}"))?;

    let validator = state
        .validators
        .get(aggregator_index)
        .ok_or_else(|| "Aggregator not found".to_string())?;

    let sync_committee_validators =
        get_sync_subcommittee_pubkeys(state, contribution.subcommittee_index);

    if !sync_committee_validators.contains(&validator.public_key) {
        return Err("The aggregator is not in the subcommittee".to_string());
    }

    let selection_data = SyncAggregatorSelectionData {
        slot: contribution.slot,
        subcommittee_index: contribution.subcommittee_index,
    };

    let selection_proof_valid = contribution_and_proof
        .selection_proof
        .verify(
            &validator.public_key,
            compute_signing_root(
                selection_data,
                state.get_domain(DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF, Some(epoch)),
            )
            .as_slice(),
        )
        .map_err(|err| format!("Selection proof verification error: {err:?}"))?;

    if !selection_proof_valid {
        return Err("The selection proof is not a valid signature".to_string());
    }

    let sync_committee_valid = contribution
        .signature
        .fast_aggregate_verify(
            sync_committee_validators
                .iter()
                .collect::<Vec<&PublicKey>>(),
            compute_signing_root(
                contribution.beacon_block_root,
                state.get_domain(DOMAIN_SYNC_COMMITTEE, Some(epoch)),
            )
            .as_ref(),
        )
        .map_err(|err| format!("Sync committee signature verification error: {err:?}"))?;

    if !sync_committee_valid {
        return Err("The aggregate signature is not valid".to_string());
    }

    let contribution_and_proof_valid = signed_contribution_and_proof
        .signature
        .verify(
            &validator.public_key,
            compute_signing_root(
                contribution_and_proof,
                state.get_domain(DOMAIN_CONTRIBUTION_AND_PROOF, Some(epoch)),
            )
            .as_slice(),
        )
        .map_err(|err| format!("Contribution and proof signature verification error: {err:?}"))?;

    if !contribution_and_proof_valid {
        return Err("The aggregator signature is not valid".to_string());
    }

    Ok(())
}

#[post("/validator/contribution_and_proofs")]
pub async fn post_contribution_and_proofs(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    event_sender: Data<broadcast::Sender<BeaconEvent>>,
    contributions: Json<Vec<SignedContributionAndProof>>,
) -> Result<impl Responder, ApiError> {
    let store = Store::new(db.get_ref().clone(), operation_pool.get_ref().clone());

    if store.is_syncing().map_err(|err| {
        ApiError::InternalError(format!("Failed to check syncing status: {err:?}"))
    })? {
        return Err(ApiError::UnderSyncing);
    }

    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| ApiError::InternalError(format!("Failed to get highest slot: {err:?}")))?
        .ok_or_else(|| ApiError::NotFound("Failed to find highest slot".to_string()))?;

    let state = get_state_from_id(ID::Slot(highest_slot), &db).await?;
    let contributions = contributions.into_inner();
    let mut failures = Vec::new();

    for (index, signed_contribution_and_proof) in contributions.iter().enumerate() {
        match validate_signed_contribution_and_proof(signed_contribution_and_proof, &state) {
            Ok(()) => {
                let event = BeaconEvent::ContributionAndProof(ContributionAndProofEvent {
                    message: signed_contribution_and_proof.message.clone(),
                    signature: signed_contribution_and_proof.signature.clone(),
                });
                let _ = event_sender.send(event);
            }
            Err(message) => {
                failures.push(ContributionAndProofFailure { index, message });
            }
        }
    }

    if !failures.is_empty() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "code": 400,
            "message": "some failures",
            "failures": failures
        })));
    }

    Ok(HttpResponse::Ok().body("success"))
}

#[derive(serde::Deserialize)]
struct BlockQuery {
    randao_reveal: BLSSignature,
    graffiti: Option<B256>,
    skip_randao_verification: Option<bool>,
    builder_boost_factor: Option<u64>,
}

#[get("/validator/blocks/{slot}")]
pub async fn get_blocks_v3(
    path: Path<u64>,
    query: Query<BlockQuery>,
    db: Data<BeaconDB>,
    node_config: Data<BeaconNodeConfig>,
) -> Result<impl Responder, ApiError> {
    let slot = path.into_inner();
    let query_params = query.into_inner();
    let randao_reveal = query_params.randao_reveal;
    let graffiti = query_params.graffiti.unwrap_or_default();
    let skip_randao_verification = query_params.skip_randao_verification.unwrap_or(true);
    let builder_boost_factor = query_params.builder_boost_factor.unwrap_or(100);

    let state = db
        .get_latest_state()
        .map_err(|err| {
            ApiError::InternalError(format!("Unable to fetch the latest state: {}", err))
        })?
        .clone();

    let current_slot = state.slot;

    if slot < current_slot {
        return Err(ApiError::BadRequest(
            "Current slot is greater than requested slot".into(),
        ));
    }

    let proposer_index = state.get_beacon_proposer_index(Some(slot)).map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get the proposer index for slot {}: {}",
            slot, err
        ))
    })?;

    let Some(proposer) = state.validators.get(proposer_index as usize) else {
        return Err(ApiError::ValidatorNotFound(format!("{proposer_index}")));
    };

    let proposer_public_key = proposer.public_key;

    if skip_randao_verification {
        if !randao_reveal.is_infinity() {
            return Err(ApiError::BadRequest("If randao verification is skipped then the randao reveal must be equal to point at infinity".into()));
        }
    } else {
        let epoch = compute_epoch_at_slot(slot);

        let randao_proof_domain = state.get_domain(DOMAIN_RANDAO, Some(epoch));
        let randao_proof_signing_root = compute_signing_root(randao_reveal, randao_proof_domain);
        if !randao_reveal
            .verify(&proposer_public_key, randao_proof_signing_root.as_ref())
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Randao reveal verification failed".to_string(),
            ));
        }
    }

    state.process_slots(slot).map_err(|err| {
        ApiError::InternalError(format!("Failed to process slots: {}", err));
    });

    let config = node_config.as_ref();

    let forkchoice_state = ForkchoiceStateV1 {
        head_block_hash: state.latest_execution_payload_header.block_hash,
        safe_block_hash: state.current_justified_checkpoint.root,
        finalized_block_hash: state.finalized_checkpoint.root,
    };

    let payload_attribute = PayloadAttributesV3 {
        timestamp: state.compute_timestamp_at_slot(slot),
        prev_randao: randao_reveal,
        suggested_fee_recipient: proposer,
        withdrawals: state.get_expected_withdrawals(),
        parent_beacon_block_root: state.latest_block_header.hash(&mut state),
    };

    let execution_engine = if let (Some(endpoint), Some(jwt_secret)) =
        (&config.execution_endpoint, &config.execution_jwt_secret)
    {
        ExecutionEngine::new(endpoint.clone(), jwt_secret.clone())?
    } else {
        return Err(ApiError::InternalError(
            "Execution endpoint or JWT secret not provided".into(),
        ));
    };

    let result = execution_engine
        .engine_forkchoice_updated_v3(forkchoice_state, Some(payload_attribute))
        .await?;

    if config.enable_builder {
    } else {
        let payload_id = result
            .payload_id
            .ok_or_else(|| format!("No payload id returned"))
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {}", err));
            });

        let payload = execution_engine.engine_get_payload_v4(payload_id.unwrap());
    }

    Ok(HttpResponse::Ok().json({}))
}

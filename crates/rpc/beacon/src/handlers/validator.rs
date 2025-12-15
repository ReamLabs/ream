use std::{collections::HashSet, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Path, Query},
};
use alloy_primitives::{Address, B256, U256};
use hashbrown::HashMap;
use ream_api_types_beacon::{
    block::{FullBlockData, ProduceBlockData, ProduceBlockResponse},
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
    attestation::Attestation,
    attester_slashing::AttesterSlashing,
    beacon_committee_selection::BeaconCommitteeSelection,
    bls_to_execution_change::SignedBLSToExecutionChange,
    deposit::Deposit,
    electra::{
        beacon_block::BeaconBlock, beacon_block_body::BeaconBlockBody, beacon_state::BeaconState,
        blinded_beacon_block::BlindedBeaconBlock,
        blinded_beacon_block_body::BlindedBeaconBlockBody, execution_payload::ExecutionPayload,
    },
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
    proposer_slashing::ProposerSlashing,
    sync_aggregate::SyncAggregate,
    sync_committe_selection::SyncCommitteeSelection,
    voluntary_exit::SignedVoluntaryExit,
};
use ream_consensus_misc::{
    attestation_data::AttestationData,
    constants::beacon::{
        DOMAIN_AGGREGATE_AND_PROOF, DOMAIN_BEACON_ATTESTER, DOMAIN_RANDAO, DOMAIN_SYNC_COMMITTEE,
        MAX_COMMITTEES_PER_SLOT, PROPOSER_REWARD_QUOTIENT, SLOTS_PER_EPOCH,
        SYNC_COMMITTEE_PROPOSER_REWARD_QUOTIENT, WHISTLEBLOWER_REWARD_QUOTIENT,
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
    rpc_types::{
        forkchoice_update::{ForkchoiceStateV1, PayloadAttributesV3},
        get_payload::PayloadV4,
    },
};
use ream_fork_choice_beacon::store::Store;
use ream_network_manager::gossipsub::validate::sync_committee_contribution_and_proof::get_sync_subcommittee_pubkeys;
use ream_operation_pool::OperationPool;
use ream_storage::{db::beacon::BeaconDB, tables::field::REDBField};
use ream_validator_beacon::{
    aggregate_and_proof::SignedAggregateAndProof,
    attestation::compute_subnet_for_attestation,
    builder::{
        builder_bid::SignedBuilderBid, builder_client::BuilderClient,
        validator_registration::SignedValidatorRegistrationV1,
    },
    constants::{
        DOMAIN_CONTRIBUTION_AND_PROOF, DOMAIN_SELECTION_PROOF,
        DOMAIN_SYNC_COMMITTEE_SELECTION_PROOF, SYNC_COMMITTEE_SUBNET_COUNT,
    },
    execution_requests::get_execution_requests,
    sync_committee::{SyncAggregatorSelectionData, is_sync_committee_aggregator},
};
use serde::Serialize;
use ssz_types::{
    VariableList,
    typenum::{U1, U8, U16},
};
use tokio::sync::broadcast;
use tree_hash::TreeHash;

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

fn verify_randao_reveal(
    state: &BeaconState,
    epoch: u64,
    randao_reveal: &BLSSignature,
    skip_randao_verification: bool,
    proposer_public_key: &PublicKey,
) -> Result<(), ApiError> {
    if skip_randao_verification {
        if !randao_reveal.is_infinity() {
            return Err(ApiError::BadRequest(
                "If randao verification is skipped then the randao reveal must be equal to point at infinity".into(),
            ));
        }
    } else {
        let randao_proof_domain = state.get_domain(DOMAIN_RANDAO, Some(epoch));
        let randao_proof_signing_root = compute_signing_root(epoch, randao_proof_domain);
        if !randao_reveal
            .verify(proposer_public_key, randao_proof_signing_root.as_ref())
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Randao reveal verification failed".to_string(),
            ));
        }
    }
    Ok(())
}

fn calculate_consensus_block_value(
    state: &BeaconState,
    attestations: &VariableList<Attestation, U8>,
    proposer_slashings: &VariableList<ProposerSlashing, U16>,
    attester_slashings: &VariableList<AttesterSlashing, U1>,
    sync_aggregate: &SyncAggregate,
) -> Result<u64, ApiError> {
    let mut total_reward = 0u64;

    // Calculate attestation rewards
    for attestation in attestations {
        if let Ok(attesting_indices) = state.get_attesting_indices(attestation) {
            let total_participating_balance: u64 = attesting_indices
                .iter()
                .filter_map(|&idx| {
                    state
                        .validators
                        .get(idx as usize)
                        .map(|v| v.effective_balance)
                })
                .sum();

            let proposer_reward = total_participating_balance
                .saturating_div(SLOTS_PER_EPOCH * PROPOSER_REWARD_QUOTIENT);

            total_reward = total_reward.saturating_add(proposer_reward);
        }
    }

    // Calculate proposer slashing rewards
    for proposer_slashing in proposer_slashings {
        let index = proposer_slashing.signed_header_1.message.proposer_index;
        if let Some(validator) = state.validators.get(index as usize) {
            total_reward += validator
                .effective_balance
                .saturating_div(WHISTLEBLOWER_REWARD_QUOTIENT);
        }
    }

    // Calculate attester slashing rewards
    let current_epoch = state.get_current_epoch();
    for attester_slashing in attester_slashings {
        if let Ok((attestation_indices_1, attestation_indices_2)) =
            state.get_slashable_attester_indices(attester_slashing)
        {
            let slashed_indices: HashSet<_> = attestation_indices_1
                .intersection(&attestation_indices_2)
                .copied()
                .collect();

            for index in slashed_indices {
                if let Some(validator) = state.validators.get(index as usize)
                    && validator.is_slashable_validator(current_epoch)
                {
                    total_reward += validator
                        .effective_balance
                        .saturating_div(WHISTLEBLOWER_REWARD_QUOTIENT);
                }
            }
        }
    }

    // Calculate sync committee rewards
    if !sync_aggregate.sync_committee_bits.is_empty() {
        let (_, base_proposer_reward) = state.get_proposer_and_participant_rewards();
        let participating_count = sync_aggregate.sync_committee_bits.num_set_bits() as u64;
        let sync_reward = participating_count
            .saturating_mul(base_proposer_reward)
            .saturating_div(SYNC_COMMITTEE_PROPOSER_REWARD_QUOTIENT);

        total_reward = total_reward.saturating_add(sync_reward);
    }

    Ok(total_reward)
}

async fn get_local_execution_payload(
    execution_engine: &ExecutionEngine,
    forkchoice_state: ForkchoiceStateV1,
    payload_attribute: PayloadAttributesV3,
) -> Result<(PayloadV4, u64), ApiError> {
    let result = execution_engine
        .engine_forkchoice_updated_v3(
            ForkchoiceStateV1 {
                head_block_hash: forkchoice_state.head_block_hash,
                safe_block_hash: forkchoice_state.safe_block_hash,
                finalized_block_hash: forkchoice_state.finalized_block_hash,
            },
            Some(PayloadAttributesV3 {
                timestamp: payload_attribute.timestamp,
                prev_randao: payload_attribute.prev_randao,
                suggested_fee_recipient: payload_attribute.suggested_fee_recipient,
                withdrawals: payload_attribute.withdrawals.clone(),
                parent_beacon_block_root: payload_attribute.parent_beacon_block_root,
            }),
        )
        .await
        .map_err(|err| ApiError::InternalError(format!("Failed to update forkchoice: {err}")))?;

    let payload_id = result.payload_id.ok_or_else(|| {
        ApiError::InternalError("No payload id returned from forkchoice update".into())
    })?;

    let payload_v4 = execution_engine
        .engine_get_payload_v4(payload_id)
        .await
        .map_err(|err| ApiError::InternalError(format!("Failed to get payload: {err}")))?;

    let execution_value: u64 = U256::from_be_bytes(payload_v4.block_value.0)
        .try_into()
        .map_err(|err| ApiError::InternalError(format!("Block value too large: {err}")))?;

    Ok((payload_v4, execution_value))
}

async fn compare_builder_vs_local(
    builder_client: Option<&Arc<BuilderClient>>,
    parent_hash: B256,
    proposer_public_key: &PublicKey,
    slot: u64,
    local_execution_value: u64,
    builder_boost_factor: u64,
) -> Result<(bool, Option<SignedBuilderBid>, u64), ApiError> {
    if let Some(builder) = builder_client {
        match builder
            .get_builder_header(parent_hash, proposer_public_key, slot)
            .await
        {
            Ok(bid) => {
                let builder_value_u256 = bid.message.value;
                let builder_value_u64: u64 = builder_value_u256.try_into().map_err(|err| {
                    ApiError::InternalError(format!("Builder value exceeds u64::MAX: {err}"))
                })?;
                let boosted_builder_value = builder_value_u64
                    .saturating_mul(builder_boost_factor)
                    .saturating_div(100);

                let use_builder = boosted_builder_value > local_execution_value;
                Ok((use_builder, Some(bid), builder_value_u64))
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to get builder header: {err:?}, falling back to local execution"
                );
                Ok((false, None, 0))
            }
        }
    } else {
        Ok((false, None, 0))
    }
}

#[get("/validator/blocks/{slot}")]
pub async fn get_blocks_v3(
    path: Path<u64>,
    query: Query<BlockQuery>,
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    execution_engine: Option<Data<ExecutionEngine>>,
    builder_client: Option<Data<Arc<BuilderClient>>>,
) -> Result<impl Responder, ApiError> {
    let slot = path.into_inner();
    let query_params = query.into_inner();
    let randao_reveal = query_params.randao_reveal;
    let graffiti = query_params.graffiti.unwrap_or_default();
    let skip_randao_verification = query_params.skip_randao_verification.unwrap_or(false);
    let builder_boost_factor = query_params.builder_boost_factor.unwrap_or(100);

    let mut state = db.get_latest_state().map_err(|err| {
        ApiError::InternalError(format!("Unable to fetch the latest state: {err}"))
    })?;

    let current_slot = state.slot;

    if slot < current_slot {
        return Err(ApiError::BadRequest(
            "Current slot is greater than requested slot".into(),
        ));
    }

    let proposer_index = state.get_beacon_proposer_index(Some(slot)).map_err(|err| {
        ApiError::InternalError(format!(
            "Failed to get the proposer index for slot {slot}: {err}",
        ))
    })?;

    let Some(proposer) = state.validators.get(proposer_index as usize) else {
        return Err(ApiError::ValidatorNotFound(format!("{proposer_index}")));
    };

    let proposer_public_key = proposer.public_key.clone();
    let epoch = compute_epoch_at_slot(slot);

    verify_randao_reveal(
        &state,
        epoch,
        &randao_reveal,
        skip_randao_verification,
        &proposer_public_key,
    )?;

    // Process slots to get state at the requested slot
    state
        .process_slots(slot)
        .map_err(|err| ApiError::InternalError(format!("Failed to process slots: {err}")))?;

    let fee_recipient = operation_pool
        .get_proposer_preparation(proposer_index)
        .unwrap_or(Address::ZERO);

    let (withdrawals, _) = state.get_expected_withdrawals().map_err(|err| {
        ApiError::InternalError(format!("Failed to get expected withdrawals: {err}"))
    })?;

    let forkchoice_state = ForkchoiceStateV1 {
        head_block_hash: state.latest_execution_payload_header.block_hash,
        safe_block_hash: state.current_justified_checkpoint.root,
        finalized_block_hash: state.finalized_checkpoint.root,
    };

    let payload_attribute = PayloadAttributesV3 {
        timestamp: state.compute_timestamp_at_slot(slot),
        prev_randao: state.get_randao_mix(epoch),
        suggested_fee_recipient: fee_recipient,
        withdrawals: withdrawals.try_into().map_err(|err| {
            ApiError::InternalError(format!(
                "Failed to convert withdrawals to VariableList: {err}"
            ))
        })?,
        parent_beacon_block_root: state.latest_block_header.tree_hash_root(),
    };

    let Some(ref execution_engine) = execution_engine else {
        return Err(ApiError::InternalError(
            "Execution engine not available".into(),
        ));
    };

    let (local_payload_v4, local_execution_value) =
        get_local_execution_payload(execution_engine, forkchoice_state, payload_attribute).await?;

    let builder_client_ref = builder_client.as_ref().map(|bc| bc.as_ref());
    let (use_builder, builder_bid, builder_value) = compare_builder_vs_local(
        builder_client_ref,
        state.latest_execution_payload_header.block_hash,
        &proposer_public_key,
        slot,
        local_execution_value,
        builder_boost_factor,
    )
    .await?;

    let proposer_slashings: VariableList<ProposerSlashing, U16> = operation_pool
        .get_all_proposer_slahsings()
        .try_into()
        .unwrap_or_default();
    let attester_slashings: VariableList<AttesterSlashing, U1> = operation_pool
        .get_all_attester_slashings()
        .try_into()
        .unwrap_or_default();
    // Attestations would need to be collected from network/gossip
    let attestations: VariableList<Attestation, U8> = VariableList::default();
    // Deposits would need to be fetched from eth1 chain
    let deposits: VariableList<Deposit, U16> = VariableList::default();
    let voluntary_exits: VariableList<SignedVoluntaryExit, U16> = operation_pool
        .get_signed_voluntary_exits()
        .try_into()
        .unwrap_or_default();
    // Sync aggregate would need to be computed from sync committee contributions
    let sync_aggregate = SyncAggregate::default();
    let bls_to_execution_changes: VariableList<SignedBLSToExecutionChange, U16> = operation_pool
        .get_signed_bls_to_execution_changes()
        .try_into()
        .unwrap_or_default();

    if use_builder {
        let builder_bid = builder_bid.expect("Builder bid should exist when use_builder is true");

        let blinded_beacon_block_body = BlindedBeaconBlockBody {
            randao_reveal: randao_reveal.clone(),
            eth1_data: state.eth1_data.clone(),
            graffiti,
            proposer_slashings: proposer_slashings.clone(),
            attester_slashings: attester_slashings.clone(),
            attestations: attestations.clone(),
            deposits: deposits.clone(),
            voluntary_exits: voluntary_exits.clone(),
            sync_aggregate: sync_aggregate.clone(),
            execution_payload_header: builder_bid.message.header.clone(),
            bls_to_execution_changes: bls_to_execution_changes.clone(),
            blob_kzg_commitments: builder_bid.message.blob_kzg_commitments.clone(),
            execution_requests: builder_bid.message.execution_requests.clone(),
        };

        let blinded_block = BlindedBeaconBlock {
            slot,
            proposer_index,
            parent_root: state.latest_block_header.tree_hash_root(),
            state_root: state.tree_hash_root(),
            body: blinded_beacon_block_body,
        };

        let consensus_block_value = calculate_consensus_block_value(
            &state,
            &attestations,
            &proposer_slashings,
            &attester_slashings,
            &sync_aggregate,
        )?;

        let response = ProduceBlockResponse {
            version: "electra".to_string(),
            execution_payload_blinded: true,
            execution_payload_value: builder_value,
            consensus_block_value,
            data: ProduceBlockData::Blinded(blinded_block),
        };

        return Ok(HttpResponse::Ok()
            .insert_header(("Eth-Consensus-Version", "electra"))
            .insert_header(("Eth-Execution-Payload-Blinded", "true"))
            .insert_header((
                "Eth-Execution-Payload-Value",
                response.execution_payload_value.to_string(),
            ))
            .insert_header((
                "Eth-Consensus-Block-Value",
                response.consensus_block_value.to_string(),
            ))
            .json(response));
    }

    let execution_payload = ExecutionPayload {
        parent_hash: local_payload_v4.execution_payload.parent_hash,
        fee_recipient: local_payload_v4.execution_payload.fee_recipient,
        state_root: local_payload_v4.execution_payload.state_root,
        receipts_root: local_payload_v4.execution_payload.receipts_root,
        logs_bloom: local_payload_v4.execution_payload.logs_bloom,
        prev_randao: local_payload_v4.execution_payload.prev_randao,
        block_number: local_payload_v4.execution_payload.block_number,
        gas_limit: local_payload_v4.execution_payload.gas_limit,
        gas_used: local_payload_v4.execution_payload.gas_used,
        timestamp: local_payload_v4.execution_payload.timestamp,
        extra_data: local_payload_v4.execution_payload.extra_data,
        base_fee_per_gas: local_payload_v4.execution_payload.base_fee_per_gas,
        block_hash: local_payload_v4.execution_payload.block_hash,
        transactions: local_payload_v4.execution_payload.transactions,
        withdrawals: local_payload_v4.execution_payload.withdrawals,
        blob_gas_used: local_payload_v4.execution_payload.blob_gas_used,
        excess_blob_gas: local_payload_v4.execution_payload.excess_blob_gas,
    };

    let blob_kzg_commitments: Vec<KZGCommitment> = local_payload_v4
        .blobs_bundle
        .commitments
        .iter()
        .filter_map(|c| {
            let vec = c.to_vec();
            if vec.len() == 48 {
                let mut bytes = [0u8; 48];
                bytes.copy_from_slice(&vec);
                Some(KZGCommitment(bytes))
            } else {
                None
            }
        })
        .collect();

    let kzg_proofs: Vec<KZGProof> = local_payload_v4
        .blobs_bundle
        .proofs
        .iter()
        .filter_map(|p| {
            let vec = p.to_vec();
            if vec.len() == 48 {
                let mut bytes = [0u8; 48];
                bytes.copy_from_slice(&vec);
                Some(KZGProof::from(bytes))
            } else {
                None
            }
        })
        .collect();

    let execution_requests =
        get_execution_requests(local_payload_v4.execution_requests.clone()).unwrap_or_default();

    let block_body = BeaconBlockBody {
        randao_reveal: randao_reveal.clone(),
        eth1_data: state.eth1_data.clone(),
        graffiti,
        proposer_slashings: proposer_slashings.clone(),
        attester_slashings: attester_slashings.clone(),
        attestations: attestations.clone(),
        deposits: deposits.clone(),
        voluntary_exits: voluntary_exits.clone(),
        sync_aggregate: sync_aggregate.clone(),
        execution_payload,
        bls_to_execution_changes: bls_to_execution_changes.clone(),
        blob_kzg_commitments: blob_kzg_commitments.try_into().unwrap_or_default(),
        execution_requests,
    };

    let block = BeaconBlock {
        slot,
        proposer_index,
        parent_root: state.latest_block_header.tree_hash_root(),
        state_root: state.tree_hash_root(),
        body: block_body,
    };

    let consensus_block_value = calculate_consensus_block_value(
        &state,
        &attestations,
        &proposer_slashings,
        &attester_slashings,
        &sync_aggregate,
    )?;

    // Note: Blobs are not included in PayloadV4 response, they would need to be fetched separately
    // For now, we return empty blobs list. The blobs would typically be fetched via
    // engine_getBlobsV1 or included in a different API response format.
    let response = ProduceBlockResponse {
        version: "electra".to_string(),
        execution_payload_blinded: false,
        execution_payload_value: local_execution_value,
        consensus_block_value,
        data: ProduceBlockData::Full(FullBlockData {
            block,
            kzg_proofs,
            blobs: Vec::new(), // Blobs not available in PayloadV4, would need separate fetch
        }),
    };

    Ok(HttpResponse::Ok()
        .insert_header(("Eth-Consensus-Version", "electra"))
        .insert_header(("Eth-Execution-Payload-Blinded", "false"))
        .insert_header((
            "Eth-Execution-Payload-Value",
            response.execution_payload_value.to_string(),
        ))
        .insert_header((
            "Eth-Consensus-Block-Value",
            response.consensus_block_value.to_string(),
        ))
        .json(response))
}

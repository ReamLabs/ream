use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json, Query},
};
use ream_api_types_beacon::{
    query::AttestationQuery,
    responses::{DataResponse, DataVersionedResponse},
};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_bls::traits::Verifiable;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    bls_to_execution_change::SignedBLSToExecutionChange, electra::beacon_state::BeaconState,
    proposer_slashing::ProposerSlashing, single_attestation::SingleAttestation,
    voluntary_exit::SignedVoluntaryExit,
};
use ream_consensus_misc::{
    constants::beacon::DOMAIN_SYNC_COMMITTEE,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use ream_network_manager::{
    gossipsub::validate::{
        beacon_attestation::validate_beacon_attestation, result::ValidationResult,
    },
    service::NetworkManagerService,
};
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::GossipMessage,
};
use ream_storage::{db::beacon::BeaconDB, tables::table::REDBTable};
use ream_validator_beacon::{
    attestation::compute_subnet_for_attestation,
    sync_committee::{SyncCommitteeMessage, is_assigned_to_sync_committee},
};
use ssz::Encode;
use ssz_types::{
    BitList, BitVector,
    typenum::{U64, U131072},
};

use crate::handlers::state::get_state_from_id;

/// GET /eth/v1/beacon/pool/bls_to_execution_changes
#[get("/beacon/pool/bls_to_execution_changes")]
pub async fn get_bls_to_execution_changes(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(
        operation_pool.get_signed_bls_to_execution_changes(),
    )))
}

/// POST /eth/v1/beacon/pool/bls_to_execution_changes
#[post("/beacon/pool/bls_to_execution_changes")]
pub async fn post_bls_to_execution_changes(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<NetworkManagerService>,
    signed_bls_to_execution_change: Json<SignedBLSToExecutionChange>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_bls_to_execution_change = signed_bls_to_execution_change.into_inner();

    beacon_state
    .validate_bls_to_execution_change(&signed_bls_to_execution_change)
    .map_err(|err| {
        ApiError::BadRequest(format!(
            "Invalid bls_to_execution_change, it will never pass validation so it's rejected: {err:?}"
        ))
    })?;

    network_manager
        .as_ref()
        .p2p_sender
        .send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::BlsToExecutionChange,
            },
            data: signed_bls_to_execution_change.as_ssz_bytes(),
        });
    operation_pool.insert_signed_bls_to_execution_change(signed_bls_to_execution_change);
    Ok(HttpResponse::Ok())
}

/// GET /eth/v1/beacon/pool/voluntary_exits
#[get("/beacon/pool/voluntary_exits")]
pub async fn get_voluntary_exits(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataResponse::new(
        operation_pool.get_signed_voluntary_exits(),
    )))
}

/// POST /eth/v1/beacon/pool/voluntary_exits
#[post("/beacon/pool/voluntary_exits")]
pub async fn post_voluntary_exits(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<NetworkManagerService>,
    signed_voluntary_exit: Json<SignedVoluntaryExit>,
) -> Result<impl Responder, ApiError> {
    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    let signed_voluntary_exit = signed_voluntary_exit.into_inner();

    beacon_state
        .validate_voluntary_exit(&signed_voluntary_exit)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid voluntary exit, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;

    network_manager
        .as_ref()
        .p2p_sender
        .send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::VoluntaryExit,
            },
            data: signed_voluntary_exit.as_ssz_bytes(),
        });

    operation_pool.insert_signed_voluntary_exit(signed_voluntary_exit);
    Ok(HttpResponse::Ok())
}

/// GET /eth/v2/beacon/pool/attester_slashings
#[get("/beacon/pool/attester_slashings")]
pub async fn get_attester_slashings(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(
        operation_pool.get_all_attester_slashings(),
    )))
}

/// POST /eth/v2/beacon/pool/attester_slashings
#[post("/beacon/pool/attester_slashings")]
pub async fn post_attester_slashings(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<Arc<NetworkManagerService>>,
    attester_slashing: Json<AttesterSlashing>,
) -> Result<impl Responder, ApiError> {
    let attester_slashing = attester_slashing.into_inner();

    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    beacon_state
        .get_slashable_attester_indices(&attester_slashing)
        .map_err(|err| {
            ApiError::BadRequest(
                format!("Invalid attester slashing, it will never pass validation so it's rejected, err: {err:?}"),
            )
        })?;
    network_manager.p2p_sender.send_gossip(GossipMessage {
        topic: GossipTopic {
            fork: beacon_state.fork.current_version,
            kind: GossipTopicKind::AttesterSlashing,
        },
        data: attester_slashing.as_ssz_bytes(),
    });

    operation_pool.insert_attester_slashing(attester_slashing);

    Ok(HttpResponse::Ok())
}

/// GET /eth/v2/beacon/pool/proposer_slashings
#[get("/beacon/pool/prposer_slashings")]
pub async fn get_proposer_slashings(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(
        operation_pool.get_all_proposer_slahsings(),
    )))
}

/// POST /eth/v2/beacon/pool/proposer_slashing
#[post("/beacon/pool/proposer_slashings")]
pub async fn post_proposer_slashings(
    db: Data<BeaconDB>,
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<Arc<NetworkManagerService>>,
    proposer_slashing: Json<ProposerSlashing>,
) -> Result<impl Responder, ApiError> {
    let proposer_slashing = proposer_slashing.into_inner();

    let highest_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get_highest_slot, error: {err:?}"))
        })?
        .ok_or(ApiError::NotFound(
            "Failed to find highest slot".to_string(),
        ))?;
    let beacon_state = get_state_from_id(ID::Slot(highest_slot), &db).await?;

    beacon_state
        .validate_proposer_slashing(&proposer_slashing)
        .map_err(|err| {
            ApiError::BadRequest(format!(
                "Invalid proposer slashing, it will never pass validation so it's rejected: {err:?}"
            ))
        })?;

    network_manager.p2p_sender.send_gossip(GossipMessage {
        topic: {
            GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::ProposerSlashing,
            }
        },
        data: proposer_slashing.as_ssz_bytes(),
    });
    operation_pool.insert_proposer_slashing(proposer_slashing);

    Ok(HttpResponse::Ok())
}

/// POST /eth/v2/beacon/pool/attestations
#[post("/beacon/pool/attestations")]
pub async fn post_attestations(
    operation_pool: Data<Arc<OperationPool>>,
    network_manager: Data<Arc<NetworkManagerService>>,
    beacon_chain: Data<Arc<BeaconChain>>,
    attestations: Json<Vec<SingleAttestation>>,
) -> Result<impl Responder, ApiError> {
    let attestations = attestations.into_inner();

    let beacon_state = get_head_state(beacon_chain.get_ref().as_ref()).await?;

    for single_attestation in attestations {
        let committees_per_slot =
            beacon_state.get_committee_count_per_slot(single_attestation.data.target.epoch);

        let subnet_id = compute_subnet_for_attestation(
            committees_per_slot,
            single_attestation.data.slot,
            single_attestation.committee_index,
        );

        match validate_beacon_attestation(
            &single_attestation,
            beacon_chain.get_ref().as_ref(),
            subnet_id,
            &network_manager.cached_db,
        )
        .await
        {
            Ok(ValidationResult::Accept) => {}
            Ok(ValidationResult::Ignore(_reason)) => {
                continue;
            }
            Ok(ValidationResult::Reject(reason)) => {
                return Err(ApiError::BadRequest(format!(
                    "Invalid attestation, rejected: {reason}"
                )));
            }
            Err(err) => {
                return Err(ApiError::InternalError(format!(
                    "Failed to validate attestation: {err:?}"
                )));
            }
        }

        let attestation = convert_single_to_attestation(&single_attestation, &beacon_state)
            .map_err(|err| ApiError::BadRequest(format!("Invalid attestation: {err:?}")))?;

        operation_pool.insert_attestation(attestation.clone(), single_attestation.committee_index);

        beacon_chain
            .process_attestation(attestation.clone(), false)
            .await
            .map_err(|err| {
                ApiError::BadRequest(format!("Attestation failed processing: {err:?}"))
            })?;

        network_manager.p2p_sender.send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: beacon_state.fork.current_version,
                kind: GossipTopicKind::BeaconAttestation(subnet_id),
            },
            data: single_attestation.as_ssz_bytes(),
        });
    }

    Ok(HttpResponse::Ok())
}

/// GET /eth/v2/beacon/pool/attestations
#[get("/beacon/pool/attestations")]
pub async fn get_attestations(
    operation_pool: Data<Arc<OperationPool>>,
    attestation_query: Query<AttestationQuery>,
) -> Result<impl Responder, ApiError> {
    let slot = attestation_query.slot;
    let committee_index = attestation_query.committee_index;
    let attestaion_data_root = attestation_query.attestation_data_root;

    let all_attestations =
        operation_pool.get_attestations(slot, committee_index, attestaion_data_root);

    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(all_attestations)))
}

fn convert_single_to_attestation(
    single: &SingleAttestation,
    state: &ream_consensus_beacon::electra::beacon_state::BeaconState,
) -> Result<Attestation, String> {
    let committee = state
        .get_beacon_committee(single.data.slot, single.committee_index)
        .map_err(|err| format!("Failed to get committee: {err:?}"))?;

    let validator_index_in_committee = committee
        .iter()
        .position(|&idx| idx == single.attester_index)
        .ok_or_else(|| format!("Validator {} not found in committee", single.attester_index))?;

    let mut aggregation_bits = BitList::<U131072>::with_capacity(committee.len())
        .map_err(|err| format!("Failed to create aggregation_bits: {err:?}"))?;
    aggregation_bits
        .set(validator_index_in_committee, true)
        .map_err(|err| format!("Failed to set aggregation bit: {err:?}"))?;

    let mut committee_bits = BitVector::<U64>::new();
    committee_bits
        .set(single.committee_index as usize, true)
        .map_err(|err| format!("Failed to set committee bit: {err:?}"))?;

    Ok(Attestation {
        aggregation_bits,
        data: single.data.clone(),
        signature: single.signature.clone(),
        committee_bits,
    })
}

async fn get_head_state(beacon_chain: &BeaconChain) -> Result<BeaconState, ApiError> {
    let store = beacon_chain.store.lock().await;

    let head_root = store
        .get_head()
        .map_err(|err| ApiError::InternalError(format!("Failed to get head root: {err:?}")))?;

    store
        .db
        .state_provider()
        .get(head_root)
        .map_err(|err| ApiError::InternalError(format!("Failed to get head state: {err:?}")))?
        .ok_or_else(|| {
            ApiError::NotFound(format!("No beacon state found for head root: {head_root}"))
        })
}

/// POST /eth/v2/beacon/pool/sync_committees
#[post("/beacon/pool/sync_committees")]
pub async fn post_sync_committees(
    messages: Json<Vec<SyncCommitteeMessage>>,
    db: Data<BeaconDB>,
) -> Result<impl Responder, ApiError> {
    for message in messages.into_inner() {
        let slot = message.slot;
        let state = get_state_from_id(ID::Slot(slot), &db).await?;

        let validator_index = message.validator_index;

        let validator = &state
            .validators
            .get(validator_index as usize)
            .ok_or(ApiError::ValidatorNotFound("Validator not found.".into()))?;

        let epoch = compute_epoch_at_slot(slot);

        let is_validator_assigned = is_assigned_to_sync_committee(&state, epoch, validator_index);
        match is_validator_assigned {
            Ok(res) => {
                if !res {
                    return Err(ApiError::BadRequest(
                        "Validator not assigned to sync committee".into(),
                    ));
                }
            }
            Err(err) => {
                return Err(ApiError::InternalError(format!(
                    "Failed due to internal error: {err}"
                )));
            }
        }

        let validator_domain = state.get_domain(DOMAIN_SYNC_COMMITTEE, Some(epoch));

        let validator_signing_root = compute_signing_root(message.slot, validator_domain);

        if !message
            .signature
            .verify(&validator.public_key, validator_signing_root.as_ref())
            .map_err(|err| {
                ApiError::InternalError(format!("Failed due to internal error: {err}"))
            })?
        {
            return Err(ApiError::BadRequest(
                "Sync committee message signature verification failed".into(),
            ));
        }
    }

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "data": "success"
    })))
}

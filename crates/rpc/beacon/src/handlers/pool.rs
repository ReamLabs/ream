use std::{collections::HashMap, sync::Arc};

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_api_types_beacon::{
    error::ApiError,
    id::ID,
    responses::{DataResponse, DataVersionedResponse},
};
use ream_bls::traits::Verifiable;
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, bls_to_execution_change::SignedBLSToExecutionChange,
    voluntary_exit::SignedVoluntaryExit,
};
use ream_consensus_misc::{
    constants::beacon::DOMAIN_SYNC_COMMITTEE,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use ream_network_manager::service::NetworkManagerService;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::GossipMessage,
};
use ream_storage::db::ReamDB;
use ream_validator_beacon::sync_committee::{
    SyncCommitteeMessage, compute_subnets_for_sync_committee, is_assigned_to_sync_committee,
};
use ssz::Encode;

use crate::handlers::state::get_state_from_id;

/// GET /eth/v1/beacon/pool/bls_to_execution_changes
#[get("/beacon/pool/bls_to_execution_changes")]
pub async fn get_bls_to_execution_changes(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_bls_to_execution_changes = operation_pool.get_signed_bls_to_execution_changes();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_bls_to_execution_changes)))
}

/// POST /eth/v1/beacon/pool/bls_to_execution_changes
#[post("/beacon/pool/bls_to_execution_changes")]
pub async fn post_bls_to_execution_changes(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
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

    operation_pool.insert_signed_bls_to_execution_change(signed_bls_to_execution_change);
    // TODO: publish bls_to_execution_change to peers (gossipsub) - https://github.com/ReamLabs/ream/issues/556

    Ok(HttpResponse::Ok())
}

/// GET /eth/v1/beacon/pool/voluntary_exits
#[get("/beacon/pool/voluntary_exits")]
pub async fn get_voluntary_exits(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    let signed_voluntary_exits = operation_pool.get_signed_voluntary_exits();
    Ok(HttpResponse::Ok().json(DataResponse::new(signed_voluntary_exits)))
}

/// POST /eth/v1/beacon/pool/voluntary_exits
#[post("/beacon/pool/voluntary_exits")]
pub async fn post_voluntary_exits(
    db: Data<ReamDB>,
    operation_pool: Data<Arc<OperationPool>>,
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

    operation_pool.insert_signed_voluntary_exit(signed_voluntary_exit);
    // TODO: publish voluntary exit to peers (gossipsub) - https://github.com/ReamLabs/ream/issues/556

    Ok(HttpResponse::Ok())
}

/// POST /eth/v1/beacon/pool/sync_committees
#[post("/beacon/pool/sync_committees")]
pub async fn post_sync_committees(
    db: Data<ReamDB>,
    network_manager: Data<Arc<NetworkManagerService>>,
    sync_committee_messages: Json<Vec<SyncCommitteeMessage>>,
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

    let sync_committee_messages = sync_committee_messages.into_inner();
    if sync_committee_messages.is_empty() {
        return Err(ApiError::BadRequest(
            "Empty sync committee messages".to_string(),
        ));
    }

    // Validate all messages and collect errors; cache subnets to reuse later
    let mut error_messages: Vec<String> = Vec::new();
    let mut subnets_by_index: HashMap<usize, _> = HashMap::new();
    for (i, sync_committee_message) in sync_committee_messages.iter().enumerate() {
        // Slot sanity check, can be relaxed if clock drift is allowed
        if sync_committee_message.slot != beacon_state.slot {
            error_messages.push(format!(
                "Sync committee message slot must match current slot: current slot={}, expected slot={}, signature={:?}",
                sync_committee_message.slot, beacon_state.slot, sync_committee_message.signature
            ));
            continue;
        }

        // verify validator is assigned to sync committee
        let epoch = compute_epoch_at_slot(sync_committee_message.slot);
        if let Err(err) = is_assigned_to_sync_committee(
            &beacon_state,
            epoch,
            sync_committee_message.validator_index,
        ) {
            error_messages.push(format!(
                "Validator is not assigned to sync committee: validator_index={}, signature={:?}, err={:?}",
                sync_committee_message.validator_index, sync_committee_message.signature, err
            ));
            continue;
        }

        // Signature verification
        let signing_root = compute_signing_root(
            sync_committee_message,
            beacon_state.get_domain(DOMAIN_SYNC_COMMITTEE, Some(epoch)),
        );
        let pubkey = match beacon_state
            .validators
            .get(sync_committee_message.validator_index as usize)
        {
            Some(validator) => &validator.public_key,
            None => {
                error_messages.push(format!(
                    "Validator with index {} not found, signature={:?}",
                    sync_committee_message.validator_index, sync_committee_message.signature
                ));
                continue;
            }
        };
        match sync_committee_message
            .signature
            .verify(pubkey, signing_root.as_ref())
        {
            Ok(true) => {}
            Ok(false) => {
                error_messages.push(format!(
                    "Invalid sync committee signature: signature={:?}",
                    sync_committee_message.signature
                ));
                continue;
            }
            Err(err) => {
                error_messages.push(format!(
                    "BLS verification error: {err:?}, signature={:?}",
                    sync_committee_message.signature
                ));
                continue;
            }
        }

        match compute_subnets_for_sync_committee(
            &beacon_state,
            sync_committee_message.validator_index,
        ) {
            Ok(subnets) => {
                subnets_by_index.insert(i, subnets);
            }
            Err(err) => {
                error_messages.push(format!(
                    "Failed to compute sync committee subnets: signature={:?}, err={:?}",
                    sync_committee_message.signature, err
                ));
                continue;
            }
        }
    }

    if !error_messages.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "One or more sync committee messages failed:\n{}",
            error_messages.join("\n")
        )));
    }

    // Gossip all messages now that validation passed for all, reusing precomputed subnets
    for (i, sync_committee_message) in sync_committee_messages.into_iter().enumerate() {
        let subnets = match subnets_by_index.get(&i) {
            Some(s) => s,
            None => {
                return Err(ApiError::InternalError(format!(
                    "precomputed subnets missing for message index {}, validator_index={}, signature={:?}",
                    i, sync_committee_message.validator_index, sync_committee_message.signature
                )));
            }
        };
        for &subnet_id in subnets {
            network_manager.p2p_sender.send_gossip(GossipMessage {
                topic: GossipTopic {
                    fork: beacon_state.fork.current_version,
                    kind: GossipTopicKind::SyncCommittee(subnet_id),
                },
                data: sync_committee_message.as_ssz_bytes(),
            });
        }
    }

    Ok(HttpResponse::Ok())
}

/// GET /eth/v2/beacon/pool/attester_slashings
#[get("/beacon/pool/attester_slashings")]
pub async fn get_pool_attester_slashings(
    operation_pool: Data<Arc<OperationPool>>,
) -> Result<impl Responder, ApiError> {
    Ok(HttpResponse::Ok().json(DataVersionedResponse::new(
        operation_pool.get_all_attester_slashings(),
    )))
}

/// POST /eth/v2/beacon/pool/attester_slashings
#[post("/beacon/pool/attester_slashings")]
pub async fn post_pool_attester_slashings(
    db: Data<ReamDB>,
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

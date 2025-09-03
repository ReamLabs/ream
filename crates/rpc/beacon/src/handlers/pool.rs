use std::sync::Arc;

use actix_web::{
    HttpResponse, Responder, get, post,
    web::{Data, Json},
};
use ream_api_types_beacon::responses::{DataResponse, DataVersionedResponse};
use ream_api_types_common::{error::ApiError, id::ID};
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, bls_to_execution_change::SignedBLSToExecutionChange,
    proposer_slashing::ProposerSlashing, voluntary_exit::SignedVoluntaryExit,
};
use ream_network_manager::service::NetworkManagerService;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::GossipMessage,
};
use ream_storage::db::ReamDB;
use ssz::Encode;

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
    db: Data<ReamDB>,
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
    db: Data<ReamDB>,
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
    db: Data<ReamDB>,
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

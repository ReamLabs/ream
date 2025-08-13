use actix_web::{
    HttpResponse, Responder, get,
    web::{Data, Path, Query},
};
use alloy_primitives::B256;
use ream_api_types_beacon::{
    error::ApiError,
    id::ID,
    query::EpochQuery,
    responses::{BeaconResponse, BeaconVersionedResponse},
};
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    checkpoint::Checkpoint, constants::beacon::SYNC_COMMITTEE_SIZE,
    misc::compute_sync_committee_period,
};
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;

pub const SYNC_COMMITTEE_SUBNET_COUNT: u64 = 4;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CheckpointData {
    previous_justified: Checkpoint,
    current_justified: Checkpoint,
    finalized: Checkpoint,
}

impl CheckpointData {
    pub fn new(
        previous_justified: Checkpoint,
        current_justified: Checkpoint,
        finalized: Checkpoint,
    ) -> Self {
        Self {
            previous_justified,
            current_justified,
            finalized,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RandaoResponse {
    pub randao: B256,
}

impl RandaoResponse {
    pub fn new(randao: B256) -> Self {
        Self { randao }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct QuotedU64Vec(#[serde(with = "serde_utils::quoted_u64_vec")] Vec<u64>);

#[derive(Serialize, Deserialize)]
struct SyncCommitteeResponse {
    #[serde(with = "serde_utils::quoted_u64_vec")]
    pub validators: Vec<u64>,
    pub validator_aggregates: Vec<QuotedU64Vec>,
}

pub async fn get_state_from_id(state_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = match state_id {
        ID::Finalized => {
            let finalized_checkpoint = db.finalized_checkpoint_provider().get().map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get finalized_checkpoint, error: {err:?}"
                ))
            })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db.justified_checkpoint_provider().get().map_err(|err| {
                ApiError::InternalError(format!(
                    "Failed to get justified_checkpoint, error: {err:?}"
                ))
            })?;

            Ok(Some(justified_checkpoint.root))
        }
        ID::Head | ID::Genesis => {
            return Err(ApiError::NotFound(format!(
                "This ID type is currently not supported: {state_id:?}"
            )));
        }
        ID::Slot(slot) => db.slot_index_provider().get(slot),
        ID::Root(root) => db.state_root_index_provider().get(root),
    }
    .map_err(|err| ApiError::InternalError(format!("Failed to get headers, error: {err:?}")))?
    .ok_or_else(|| ApiError::NotFound(format!("Failed to find `block_root` from {state_id:?}")))?;

    db.beacon_state_provider()
        .get(block_root)
        .map_err(|err| {
            ApiError::InternalError(format!("Failed to get block by block_root, error: {err:?}"))
        })?
        .ok_or_else(|| ApiError::NotFound(format!("Failed to find `block_root` from {state_id:?}")))
}

#[get("/beacon/states/{state_id}/root")]
pub async fn get_state_root(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    let state_root = state.tree_hash_root();

    Ok(HttpResponse::Ok().json(BeaconResponse::new(state_root)))
}

/// Called by `/eth/v1/beacon/states/{state_id}/fork` to get fork of state.
#[get("/beacon/states/{state_id}/fork")]
pub async fn get_state_fork(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(HttpResponse::Ok().json(BeaconResponse::new(state.fork)))
}

/// Called by `/states/<state_id>/finality_checkpoints` to get the Checkpoint Data of state.
#[get("/beacon/states/{state_id}/finality_checkpoints")]
pub async fn get_state_finality_checkpoint(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconResponse::new(CheckpointData::new(
            state.previous_justified_checkpoint,
            state.current_justified_checkpoint,
            state.finalized_checkpoint,
        ))),
    )
}

/// Called by `/states/<state_id>/randao` to get the Randao mix of state.
/// Pass optional `epoch` in the query to get randao for particular epoch,
/// else will fetch randao of the state epoch
#[get("/beacon/states/{state_id}/randao")]
pub async fn get_state_randao(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    query: Query<EpochQuery>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    let randao_mix = match query.epoch {
        Some(epoch) => state.get_randao_mix(epoch),
        None => state.get_randao_mix(state.get_current_epoch()),
    };

    Ok(HttpResponse::Ok().json(BeaconResponse::new(RandaoResponse::new(randao_mix))))
}

/// Called by `/eth/v1/beacon/states/{state_id}/pending_consolidations` to get pending
/// consolidations for state with given stateId
#[get("/beacon/states/{state_id}/pending_consolidations")]
pub async fn get_pending_consolidations(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconVersionedResponse::new(Vec::from(
            state.pending_consolidations,
        ))),
    )
}

/// Called by `/eth/v1/beacon/states/{state_id}/pending_deposits` to get pending deposits
/// for state with given stateId
#[get("/beacon/states/{state_id}/pending_deposits")]
pub async fn get_pending_deposits(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconVersionedResponse::new(Vec::from(
            state.pending_deposits,
        ))),
    )
}

/// Called by `/states/{state_id}/pending_partial_withdrawals` to get pending partial withdrawals
/// for state with given stateId
#[get("/beacon/states/{state_id}/pending_partial_withdrawals")]
pub async fn get_pending_partial_withdrawals(
    db: Data<ReamDB>,
    state_id: Path<ID>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;

    Ok(
        HttpResponse::Ok().json(BeaconVersionedResponse::new(Vec::from(
            state.pending_partial_withdrawals,
        ))),
    )
}

/// Called by `/states/{state_id}/sync_committees` to get sync_committees
/// for state with given `stateId`.
/// will use `epoch` if provided.
#[get("/beacon/states/{state_id}/sync_committees")]
pub async fn get_sync_committees(
    db: Data<ReamDB>,
    state_id: Path<ID>,
    epoch: Query<EpochQuery>,
) -> Result<impl Responder, ApiError> {
    let state = get_state_from_id(state_id.into_inner(), &db).await?;
    let current_epoch = state.get_current_epoch();
    let epoch = epoch.epoch.unwrap_or(current_epoch);
    let sync_committee_period = compute_sync_committee_period(epoch);
    let current_sync_committee_period = compute_sync_committee_period(current_epoch);

    let sync_committee = if sync_committee_period == current_sync_committee_period {
        &state.current_sync_committee
    } else if sync_committee_period == current_sync_committee_period + 1 {
        &state.next_sync_committee
    } else {
        return Err(ApiError::BadRequest(format!(
            "state at epoch {current_epoch} has no sync committee for epoch {epoch}"
        )));
    };

    let validators = sync_committee
        .public_keys
        .iter()
        .filter_map(|public_key| {
            state
                .validators
                .iter()
                .position(|validator| validator.public_key == *public_key)
                .map(|position| position as u64)
        })
        .collect::<Vec<_>>();

    let validator_aggregates = validators
        .chunks_exact((SYNC_COMMITTEE_SIZE / SYNC_COMMITTEE_SUBNET_COUNT) as usize)
        .map(|chunk| QuotedU64Vec(chunk.to_vec()))
        .collect::<Vec<QuotedU64Vec>>();

    Ok(
        HttpResponse::Ok().json(BeaconVersionedResponse::new(SyncCommitteeResponse {
            validators,
            validator_aggregates,
        })),
    )
}

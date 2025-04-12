use ream_consensus::deneb::beacon_state::BeaconState;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply,with_status},
};

use crate::types::{
    errors::ApiError,
    id::ID,
    response::{BeaconResponse, BeaconVersionedResponse, RootResponse},
};

pub async fn get_state_from_id(state_id: ID, db: &ReamDB) -> Result<BeaconState, ApiError> {
    let block_root = match state_id {
        ID::Finalized => {
            let finalized_checkpoint = db
                .finalized_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Finalized checkpoint not found"))
                })?;

            Ok(Some(finalized_checkpoint.root))
        }
        ID::Justified => {
            let justified_checkpoint = db
                .justified_checkpoint_provider()
                .get()
                .map_err(|_| ApiError::InternalError)?
                .ok_or_else(|| {
                    ApiError::NotFound(String::from("Justified checkpoint not found"))
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
    .map_err(|_| ApiError::InternalError)?
    .ok_or(ApiError::NotFound(format!(
        "Failed to find `block_root` from {state_id:?}"
    )))?;

    db.beacon_state_provider()
        .get(block_root)
        .map_err(|_| ApiError::InternalError)?
        .ok_or(ApiError::NotFound(format!(
            "Failed to find `beacon_state` from {block_root:?}"
        )))
}

pub async fn get_state(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    Ok(with_status(BeaconResponse::json(state), StatusCode::OK))
}

pub async fn get_state_root(state_id: ID, db: ReamDB) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    let state_root = state.tree_hash_root();

    Ok(with_status(
        BeaconResponse::json(RootResponse::new(state_root)),
        StatusCode::OK,
    ))
}
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WithdrawalData {
    #[serde(with = "serde_utils::quoted_u64")]
    validator_index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    amount: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    withdrawable_epoch: u64,
}
// Called by `/states/{state_id}/get_pending_partial_withdrawals` to get pending partial withdrawals for state with given stateId
pub async fn get_pending_partial_withdrawals(
    state_id: ID,
    db: ReamDB,
) -> Result<impl Reply, Rejection> {
    let state = get_state_from_id(state_id, &db).await?;

    let withdrawals = state.get_expected_withdrawals();
    let withdrawal_data: Vec<WithdrawalData> = withdrawals
        .into_iter()
        .map(|w| WithdrawalData {
            validator_index: w.validator_index,
            amount: w.amount,
            withdrawable_epoch: state.get_current_epoch(),
        })
        .collect();
    let response = warp::reply::with_header(
        BeaconVersionedResponse::json(withdrawal_data),
        "Eth-Consensus-Version",
        "electra",
    );

    Ok(with_status(response, StatusCode::OK))
}

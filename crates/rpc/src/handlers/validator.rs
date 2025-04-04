use std::sync::Arc;

use ream_bls::pubkey::pubkey_from_str;
use ream_storage::db::ReamDB;
use serde_json::json;
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{self, Reply, with_status},
};

use super::{BeaconResponse, state::get_state_from_id};

pub async fn get_validator_from_state(
    state_id: String,
    validator_id: String,
    db: Arc<ReamDB>,
) -> Result<impl Reply, Rejection> {
    let validator_key = match pubkey_from_str(&validator_id) {
        Ok(key) => key,
        Err(_) => {
            let error_body = reply::json(&json!({
                "code": 400,
                "message": format!("Invalid validator ID: {}", validator_id),
            }));
            return Ok(with_status(error_body, StatusCode::BAD_REQUEST));
        }
    };

    let state = get_state_from_id(state_id, db).await?;

    if let Some((index, validator)) = state
        .validators
        .iter()
        .enumerate()
        .find(|(_, v)| v.pubkey == validator_key)
    {
        let balance = state
            .balances
            .get(index)
            .expect("Unable to fetch validator balance");
        let validator_data = json!({"index":index,"balance":balance,"status":"active_ongoing","validator":validator});

        Ok(with_status(
            BeaconResponse::json(validator_data),
            StatusCode::OK,
        ))
    } else {
        let error_body = reply::json(&json!({
            "code": 400,
            "message": format!("Validator not found: {}", validator_id),
        }));
        Ok(with_status(error_body, StatusCode::NOT_FOUND))
    }
}

use warp::{
    http::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use crate::types::errors::ApiError;

pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
    if err.is_not_found() {
        return Ok(with_status("NOT FOUND".to_string(), StatusCode::NOT_FOUND));
    }

    if let Some(api_error) = err.find::<ApiError>() {
        let (message, code) = match api_error {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message.to_string()),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message.to_string()),
        };
        return Ok(with_status(code, message));
    }

    Ok(with_status(
        "INTERNAL SERVER ERROR".to_string(),
        StatusCode::INTERNAL_SERVER_ERROR,
    ))
}

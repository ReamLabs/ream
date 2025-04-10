use warp::{
    http::StatusCode,
    reject::Rejection,
    reply::{Reply, json, with_status},
};

use crate::types::errors::{ApiError, ErrorMessage};
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
    if err.is_not_found() {
        return Ok(with_status(
            json(&ErrorMessage {
                code: 404,
                message: "Not Found".to_string(),
            }),
            StatusCode::NOT_FOUND,
        ));
    }

    if let Some(api_error) = err.find::<ApiError>() {
        let (status, message) = match api_error {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_string(),
            ),
            ApiError::InvalidParameter(msg) => (
                StatusCode::BAD_REQUEST,
                format!("Invalid state ID: {}", msg),
            ),
            ApiError::ValidatorNotFound(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        };

        return Ok(with_status(
            json(&ErrorMessage {
                code: status.as_u16(),
                message,
            }),
            status,
        ));
    }

    Ok(with_status(
        json(&ErrorMessage {
            code: 500,
            message: "Internal Server Error".to_string(),
        }),
        StatusCode::INTERNAL_SERVER_ERROR,
    ))
}

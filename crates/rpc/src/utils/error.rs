use warp::{reject::Rejection, reply::Reply};

use crate::types::errors::ApiError;

pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
    let (code, message) = if err.is_not_found() {
        (warp::http::StatusCode::NOT_FOUND, "NOT FOUND".to_string())
    } else if let Some(api_error) = err.find::<ApiError>() {
        match api_error {
            ApiError::Unauthorized => (
                warp::http::StatusCode::UNAUTHORIZED,
                "Unauthorized".to_string(),
            ),
            ApiError::NotFound(msg) => (warp::http::StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(bd) => (warp::http::StatusCode::BAD_REQUEST, bd.to_string()),
        }
    } else {
        (
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL SERVER ERROR".to_string(),
        )
    };
    Ok(warp::reply::with_status(message, code))
}

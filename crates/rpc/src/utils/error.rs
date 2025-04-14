use std::{error::Error, str::FromStr};

use warp::{
    Filter,
    filters::body::BodyDeserializeError,
    http::StatusCode,
    reject::{Rejection, custom},
    reply::{Reply, json, with_status},
};

use crate::types::errors::{ApiError, ErrorMessage};
pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Rejection> {
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
            ApiError::TooManyValidatorsIds() => (
                StatusCode::URI_TOO_LONG,
                "Too many validator IDs in request".to_string(),
            ),
        };

        return Ok(with_status(
            json(&ErrorMessage {
                code: status.as_u16(),
                message,
            }),
            status,
        ));
    }

    if let Some(e) = err.find::<BodyDeserializeError>() {
        if let Some(source) = e.source() {
            return Ok(with_status(
                json(&ErrorMessage {
                    code: 400,
                    message: format!("{}", source),
                }),
                StatusCode::BAD_REQUEST,
            ));
        }

        return Ok(with_status(
            json(&ErrorMessage {
                code: 400,
                message: format!("{}", e),
            }),
            StatusCode::BAD_REQUEST,
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

pub fn parsed_param<T>() -> impl Filter<Extract = (T,), Error = Rejection> + Clone
where
    T: FromStr<Err = ApiError> + Send + 'static,
{
    warp::path::param::<String>().and_then(|s: String| async move {
        match s.parse::<T>() {
            Ok(val) => Ok(val),
            Err(e) => Err(custom(e)),
        }
    })
}

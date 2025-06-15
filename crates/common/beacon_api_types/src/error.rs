use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Api Endpoint Not Found: {0}")]
    NotFound(String),

    #[error("Bad Request: {0}")]
    BadRequest(String),

    #[error("Internal Server Error")]
    InternalError,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Validator not found: {0}")]
    ValidatorNotFound(String),

    #[error("Too many validator IDs in request")]
    TooManyValidatorsIds,
}

impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            ApiError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::InvalidParameter(_) => StatusCode::BAD_REQUEST,
            ApiError::ValidatorNotFound(_) => StatusCode::NOT_FOUND,
            ApiError::TooManyValidatorsIds => StatusCode::URI_TOO_LONG,
        }
    }
}

#[derive(Debug, Error)]
pub enum ValidatorError {
    #[error("Request failed with status code: {status_code}")]
    RequestFailed { status_code: reqwest::StatusCode },

    #[error("Failed to decode SSZ response: {0}")]
    SszDecodeError(String),

    #[error("Failed to decode JSON response: {0}")]
    JsonDecodeError(String),

    #[error("HTTP client error: {0}")]
    HttpClientError(#[from] reqwest::Error),

    #[error("URL parsing error: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Invalid content type: expected application/octet-stream or application/json")]
    InvalidContentType,

    #[error("Network timeout")]
    Timeout,

    #[error("Invalid response format")]
    InvalidResponse,

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

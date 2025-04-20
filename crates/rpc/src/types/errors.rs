use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use derive_more::derive::Display;
use serde::{Deserialize, Serialize};

#[derive(Debug, Display)]
pub enum ApiError {
    #[display("Unauthorized")]
    Unauthorized,

    #[display("Api Endpoint Not Found: {_0}")]
    NotFound(String),

    #[display("Bad Request: {_0}")]
    BadRequest(String),

    #[display("Internal Server Error")]
    InternalError,

    #[display("Invalid parameter: {_0}")]
    InvalidParameter(String),

    #[display("Validator not found: {_0}")]
    ValidatorNotFound(String),
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
        }
    }
}

// impl Reject for ApiError {}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorMessage {
    pub code: u16,
    pub message: String,
}

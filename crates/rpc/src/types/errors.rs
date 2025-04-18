use actix_web::{HttpResponse, ResponseError};
use serde::{Deserialize, Serialize};
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

// impl ResponseError trait allows to convert our errors into http responses with appropriate data
impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::TooManyValidatorsIds => {
                HttpResponse::InternalServerError().json("Too many validator IDs in request")
            }
            _ => {
                todo!()
            }
        }
    }
}

// impl Reject for ApiError {}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorMessage {
    pub code: u16,
    pub message: String,
}

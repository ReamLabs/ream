use thiserror::Error;
use warp::reject::Reject;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("Api Endpoint Not Found: {0}")]
    NotFound(String),

    #[error("Bad Request: {0}")]
    BadRequest(String),
}

impl Reject for ApiError {}

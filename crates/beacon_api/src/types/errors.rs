use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Unauthorized")]
    Unauthorized,

    #[error("{0}")]
    NotFound(String),

    #[error("Bad Request: {0}")]
    BadRequest(String),
}


impl warp::reject::Reject for ApiError{}
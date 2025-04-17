use actix_web::{HttpResponse, error::ResponseError};
use derive_more::Display;

#[derive(Debug, Display)]
pub enum RpcError {
    #[display("Internal Server Error")]
    InternalServerError,

    #[display("BadRequest: {_0}")]
    BadRequest(String),

    #[display("Unauthorized")]
    Unauthorized,
}

// impl ResponseError trait allows to convert our errors into http responses with appropriate data
impl ResponseError for RpcError {
    fn error_response(&self) -> HttpResponse {
        match self {
            Self::InternalServerError => {
                HttpResponse::InternalServerError().json("Internal Server Error, Please try later")
            }
            Self::BadRequest(message) => HttpResponse::BadRequest().json(message),
            Self::Unauthorized => HttpResponse::Unauthorized().json("Unauthorized"),
        }
    }
}

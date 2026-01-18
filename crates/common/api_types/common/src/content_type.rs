use actix_web::http::header::HeaderValue as ActixHeaderValue;
use reqwest::header::HeaderValue as ReqwestHeaderValue;

pub const JSON_ACCEPT_PRIORITY: &str = "application/json;q=1";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const SSZ_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone)]
pub enum ContentType {
    Json,
    Ssz,
}

impl ContentType {
    pub fn to_header_value(&self) -> ReqwestHeaderValue {
        match self {
            ContentType::Json => ReqwestHeaderValue::from_static(JSON_CONTENT_TYPE),
            ContentType::Ssz => ReqwestHeaderValue::from_static(SSZ_CONTENT_TYPE),
        }
    }
}

impl From<Option<&ActixHeaderValue>> for ContentType {
    fn from(header_value: Option<&ActixHeaderValue>) -> Self {
        match header_value.and_then(|h| h.to_str().ok()) {
            Some(s) if s.split(';').any(|p| p.trim() == SSZ_CONTENT_TYPE) => ContentType::Ssz,
            _ => ContentType::Json,
        }
    }
}

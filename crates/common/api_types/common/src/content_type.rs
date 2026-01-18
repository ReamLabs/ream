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

impl From<&ActixHeaderValue> for ContentType {
    fn from(header_value: &ActixHeaderValue) -> Self {
        header_value
            .to_str()
            .map(|s| {
                if s.contains(SSZ_CONTENT_TYPE) {
                    ContentType::Ssz
                } else {
                    ContentType::Json
                }
            })
            .unwrap_or(ContentType::Json)
    }
}

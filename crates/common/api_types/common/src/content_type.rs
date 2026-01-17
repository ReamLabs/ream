use reqwest::header::HeaderValue;

pub const JSON_ACCEPT_PRIORITY: &str = "application/json;q=1";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const SSZ_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone)]
pub enum ContentType {
    Json,
    Ssz,
}

impl ContentType {
    pub fn to_header_value(&self) -> HeaderValue {
        match self {
            ContentType::Json => HeaderValue::from_static(JSON_CONTENT_TYPE),
            ContentType::Ssz => HeaderValue::from_static(SSZ_CONTENT_TYPE),
        }
    }
}

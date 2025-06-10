use std::time::Duration;

use anyhow::anyhow;
use reqwest::{
    Client, IntoUrl, Request, RequestBuilder, Response, Url,
    header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue},
};

pub const ACCEPT_PRIORITY: &str = "application/octet-stream;q=1.0,application/json;q=0.9";
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

#[derive(Debug, Clone)]
pub struct ClientWithBaseUrl {
    client: Client,
    base_url: Url,
    content_type: ContentType,
}

impl ClientWithBaseUrl {
    pub fn new(
        url: Url,
        request_timeout: Duration,
        content_type: ContentType,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(|err| anyhow!("Failed to build HTTP client {err:?}"))?;

        Ok(Self {
            client,
            base_url: url,
            content_type,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn get<U: IntoUrl>(&self, url: U) -> anyhow::Result<RequestBuilder> {
        let url = self.base_url.join(url.as_str())?;

        let mut headers = HeaderMap::new();
        match self.content_type {
            ContentType::Json => {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE));
            }
            ContentType::Ssz => {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static(SSZ_CONTENT_TYPE));
                headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_PRIORITY));
            }
        }

        Ok(self.client.get(url).headers(headers))
    }

    pub fn post<U: IntoUrl>(
        &self,
        url: U,
        content_type: ContentType,
    ) -> anyhow::Result<RequestBuilder> {
        let url = self.base_url.join(url.as_str())?;

        Ok(self
            .client
            .post(url)
            .header(CONTENT_TYPE, content_type.to_header_value()))
    }

    pub async fn execute(&self, request: Request) -> Result<Response, reqwest::Error> {
        self.client.execute(request).await
    }
}

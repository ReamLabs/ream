use std::time::Duration;

use anyhow::anyhow;
use reqwest::{
    Client, IntoUrl, Request, RequestBuilder, Response, Url,
    header::{ACCEPT, CONTENT_TYPE, HeaderValue},
};

pub const ACCEPT_PRIORITY: &str = "application/octet-stream;q=1.0,application/json;q=0.9";
pub const JSON_ACCEPT_PRIORITY: &str = "application/json;q=1";
pub const JSON_CONTENT_TYPE: &str = "application/json";
pub const SSZ_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Debug, Clone, Copy)]
pub enum ContentType {
    Json,
    Ssz,
}

#[derive(Debug, Clone)]
pub struct ClientWithBaseUrl {
    client: Client,
    base_url: Url,
}

impl ClientWithBaseUrl {
    pub fn new(url: Url, request_timeout: Duration) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(|err| anyhow!("Failed to build HTTP client {err:?}"))?;

        Ok(Self {
            client,
            base_url: url,
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
        Ok(self
            .client
            .get(url)
            .header(CONTENT_TYPE, HeaderValue::from_static(SSZ_CONTENT_TYPE))
            .header(ACCEPT, HeaderValue::from_static(ACCEPT_PRIORITY)))
    }

    pub fn post<U: IntoUrl>(&self, url: U) -> anyhow::Result<RequestBuilder> {
        let url = self.base_url.join(url.as_str())?;
        Ok(self
            .client
            .post(url)
            .header(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE))
            .header(ACCEPT, HeaderValue::from_static(ACCEPT_PRIORITY)))
    }

    pub async fn execute(&self, request: Request) -> Result<Response, reqwest::Error> {
        self.client.execute(request).await
    }
}

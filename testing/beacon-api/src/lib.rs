use std::error::Error;
use std::fs;
use std::path::Path;
use reqwest::{Client, StatusCode};
use serde::{Deserialize};
use std::collections::HashMap;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct OpenApiFile {
    pub url: String,
    pub filepath: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenApiJson {
    pub paths: HashMap<String, HashMap<String, serde_json::Value>>,
    pub info: Info,
}

#[derive(Debug, Deserialize)]
pub struct Info {
    pub version: String,
}

#[derive(Debug, Clone)]
pub enum ApiError {
    NotFound(String),
    InvalidInput(String),
    Http(StatusCode, String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            ApiError::Http(status, msg) => write!(f, "HTTP error {}: {}", status, msg),
        }
    }
}

impl Error for ApiError {}

pub async fn fetch_openapi_spec(openapi: &OpenApiFile) -> Result<OpenApiJson, Box<dyn Error>> {
    if Path::new(&openapi.filepath).exists() {
        let content = fs::read_to_string(&openapi.filepath)?;
        let api_json: OpenApiJson = serde_json::from_str(&content)?;
        
        if extract_version(&api_json.info.version) == extract_version(&openapi.version) {
            return Ok(api_json);
        }
    }

    println!("Downloading OpenAPI spec from {}", openapi.url);
    let client = Client::new();
    let resp = client.get(&openapi.url).send().await?;
    
    if !resp.status().is_success() {
        return Err(Box::new(ApiError::Http(resp.status(), "Request returned error status".to_string())));
    }
    
    let text = resp.text().await?;
    
    if let Some(parent) = Path::new(&openapi.filepath).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&openapi.filepath, &text)?;

    let api_json: OpenApiJson = serde_json::from_str(&text)?;
    
    if extract_version(&api_json.info.version) != extract_version(&openapi.version) {
        return Err(Box::new(ApiError::InvalidInput(format!(
            "Downloaded OpenAPI file version {} doesn't match expected {}",
            api_json.info.version,
            openapi.version
        ))));
    }

    Ok(api_json)
}

fn extract_version(version: &str) -> String {
    let version_regex = Regex::new(r"v?(\d+\.\d+\.\d+)").unwrap();
    version_regex.captures(version)
        .map(|cap| cap[1].to_string())
        .unwrap_or_else(|| version.to_string())
}

pub mod schema;
pub mod test_utils;
pub mod beacon_api;
pub mod test_data; 
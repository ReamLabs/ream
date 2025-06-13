use beacon_api_tests::beacon_api::BeaconApi;
use beacon_api_tests::schema::SchemaValidator;
use beacon_api_tests::test_utils::TestClient;
use beacon_api_tests::{fetch_openapi_spec, OpenApiFile};
use serde_json::Value;
use std::error::Error;

const SCHEMA_URL: &str = "https://github.com/ethereum/beacon-APIs/releases/download/v2.5.0/beacon-node-oapi.json";
const SCHEMA_PATH: &str = "schemas/beacon_api.json";
const SCHEMA_VERSION: &str = "2.5.0";
const BASE_URL: &str = "http://localhost:5052";

#[tokio::test]
async fn test_get_genesis() -> Result<(), Box<dyn Error>> {
    let openapi = OpenApiFile {
        url: SCHEMA_URL.to_string(),
        filepath: SCHEMA_PATH.to_string(),
        version: SCHEMA_VERSION.to_string(),
    };

    if !std::path::Path::new(SCHEMA_PATH).exists() {
        fetch_openapi_spec(&openapi).await?;
    }

    let client = TestClient::new(BASE_URL.to_string(), SCHEMA_PATH)?;
    let api = BeaconApi::new(BASE_URL);
    let response = client.get(&api.get_genesis_url()).await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_state_root() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getStateRoot").await?;
    println!("State root response: {}", serde_json::to_string_pretty(&response)?);
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_fork() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getFork").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_finality_checkpoints() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getFinalityCheckpoints").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_epoch_committees() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getEpochCommittees").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_block_header() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getBlockHeader").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_block() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getBlock").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_block_root() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getBlockRoot").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_block_attestations() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getBlockAttestations").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_attestations() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getAttestations").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_attester_slashings() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getAttesterSlashings").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_proposer_slashings() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getProposerSlashings").await?;
    assert!(response.get("data").is_some());
    Ok(())
}

#[tokio::test]
async fn test_get_block_headers() -> Result<(), Box<dyn Error>> {
    let client = TestClient::new(
        "http://localhost:5052".to_string(),
        "schemas/beacon_api.json"
    )?;

    let response = client.get("getBlockHeaders").await?;
    let data = response.get("data").expect("Response should have data field");
    assert!(data.get("root").is_some());
    assert!(data.get("canonical").is_some());
    assert!(data.get("header").is_some());
    let header = data.get("header").unwrap();
    assert!(header.get("message").is_some());
    assert!(header.get("signature").is_some());
    let message = header.get("message").unwrap();
    assert!(message.get("slot").is_some());
    assert!(message.get("proposer_index").is_some());
    assert!(message.get("parent_root").is_some());
    assert!(message.get("state_root").is_some());
    assert!(message.get("body_root").is_some());
    Ok(())
} 
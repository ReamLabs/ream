use beacon_api_tests::{test_utils::TestClient, schema::SchemaValidator};
use serde_json::{json, Value};
use std::error::Error;
use beacon_api_tests::test_data::get_test_data;
use std::fs;

const SCHEMA_PATH: &str = "schemas/beacon_api.json";

fn assert_json_eq(actual: &Value, expected: &Value) {
    assert_eq!(actual, expected, "JSON values do not match");
}

#[tokio::test]
async fn test_get_genesis() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getGenesis").await?;
    validator.validate_response("getGenesis", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_state_root() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getStateRoot", args).await?;
    validator.validate_response("getStateRoot", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_state_fork() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getStateFork", args).await?;
    validator.validate_response("getStateFork", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_state_finality_checkpoints() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getStateFinalityCheckpoints", args).await?;
    validator.validate_response("getStateFinalityCheckpoints", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_validators() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "id": ["0"]
    });
    let response = client.get_with_args("getValidators", args).await?;
    validator.validate_response("getValidators", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_validator_balances() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "id": ["0"]
    });
    let response = client.get_with_args("getValidatorBalances", args).await?;
    validator.validate_response("getValidatorBalances", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_committees() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getCommittees", args).await?;
    validator.validate_response("getCommittees", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_headers() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getBlockHeaders").await?;
    validator.validate_response("getBlockHeaders", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockV2", args).await?;
    validator.validate_response("getBlockV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_blinded_block() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlindedBlock", args).await?;
    validator.validate_response("getBlindedBlock", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_attestations_v2() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockAttestationsV2", args).await?;
    validator.validate_response("getBlockAttestationsV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_header() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockHeader", args).await?;
    validator.validate_response("getBlockHeader", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_root() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockRoot", args).await?;
    validator.validate_response("getBlockRoot", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_blob_sidecars() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlobSidecars", args).await?;
    validator.validate_response("getBlobSidecars", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_attestations() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolAttestations").await?;
    validator.validate_response("getPoolAttestations", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_attestations_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolAttestationsV2").await?;
    validator.validate_response("getPoolAttestationsV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_attester_slashings_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolAttesterSlashingsV2").await?;
    validator.validate_response("getPoolAttesterSlashingsV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_proposer_slashings_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolProposerSlashingsV2").await?;
    validator.validate_response("getPoolProposerSlashingsV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_voluntary_exits_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolVoluntaryExitsV2").await?;
    validator.validate_response("getPoolVoluntaryExitsV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pool_bls_to_execution_changes_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let response = client.get("getPoolBLSToExecutionChangesV2").await?;
    validator.validate_response("getPoolBLSToExecutionChangesV2", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_state_randao() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "epoch": 0
    });
    let response = client.get_with_args("getStateRandao", args).await?;
    validator.validate_response("getStateRandao", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_epoch_sync_committees() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "epoch": 0
    });
    let response = client.get_with_args("getEpochSyncCommittees", args).await?;
    validator.validate_response("getEpochSyncCommittees", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pending_deposits() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getPendingDeposits", args).await?;
    validator.validate_response("getPendingDeposits", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pending_partial_withdrawals() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getPendingPartialWithdrawals", args).await?;
    validator.validate_response("getPendingPartialWithdrawals", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_pending_consolidations() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getPendingConsolidations", args).await?;
    validator.validate_response("getPendingConsolidations", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_proposer_lookahead() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head"
    });
    let response = client.get_with_args("getProposerLookahead", args).await?;
    validator.validate_response("getProposerLookahead", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_rewards() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockRewards", args).await?;
    validator.validate_response("getBlockRewards", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_attestations_rewards() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "epoch": 0
    });
    let response = client.get_with_args("getAttestationsRewards", args).await?;
    validator.validate_response("getAttestationsRewards", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_sync_committee_rewards() -> Result<(), Box<dyn Error>> {
    let schema = std::fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "state_id": "head",
        "epoch": 0
    });
    let response = client.get_with_args("getSyncCommitteeRewards", args).await?;
    validator.validate_response("getSyncCommitteeRewards", &response)?;
    Ok(())
}

#[tokio::test]
async fn test_get_block_v2() -> Result<(), Box<dyn Error>> {
    let schema = fs::read_to_string(SCHEMA_PATH)?;
    let schema: Value = serde_json::from_str(&schema)?;
    
    let client = TestClient::new(SCHEMA_PATH)?;
    let validator = SchemaValidator::new(schema)?;

    let args = json!({
        "block_id": "head"
    });
    let response = client.get_with_args("getBlockV2", args).await?;
    validator.validate_response("getBlockV2", &response)?;
    Ok(())
}
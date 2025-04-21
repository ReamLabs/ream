use std::env;

use alloy_primitives::b256;
use ream_consensus::deneb::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState};
use ream_rpc::types::response::BeaconVersionedResponse;

#[tokio::test]
pub async fn test_serialization() {
    // Load a RPC URL in your terminal ENV. eg: Quicknode
    let rpc_url = env::var("RPC").expect("RPC URL must be set in the environment");
    let _ = test_beacon_state_serialization(&rpc_url).await;
    let _ = test_beacon_block_serialization(&rpc_url, 11532800).await;
}

pub async fn test_beacon_state_serialization(rpc_url: &str) -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<BeaconState>;
    let body=reqwest::get(format!("{}/eth/v2/debug/beacon/states/0x78329cf91573da18accec7f7eb665a482dff15a8134b71b2a6c4b79cc5d051c3",rpc_url)).await?.text().await?;

    println!("Serialization initiated");
    let beacon_state: Response = serde_json::from_str(&body).unwrap();

    assert_eq!(beacon_state.version, "deneb");
    assert_eq!(beacon_state.data.latest_block_header.slot, 11532800);
    assert_eq!(
        beacon_state.data.latest_block_header.parent_root,
        b256!("0x81c89d2dbd540ade21b9c28d8a78395706563ac5af78fb67c3960ecad3706c8a")
    );

    let re_encoded = serde_json::to_string(&beacon_state)?;
    let reparsed: serde_json::Value = serde_json::from_str(&re_encoded)?;
    let original: serde_json::Value = serde_json::from_str(&body)?;

    assert_eq!(
        reparsed, original,
        "Re-encoded state doesn't match original JSON"
    );

    println!("State Serialization completed successfully");

    Ok(())
}
pub async fn test_beacon_block_serialization(rpc_url: &str, slot: u64) -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<SignedBeaconBlock>;
    let body = reqwest::get(format!("{}/eth/v2/beacon/blocks/{}", rpc_url, slot))
        .await?
        .text()
        .await?;

    println!("Block Serialization initiated");
    let beacon_block: Response = serde_json::from_str(&body).unwrap();

    assert_eq!(beacon_block.version, "deneb");
    assert_eq!(beacon_block.data.message.slot, 11532800);

    let re_encoded = serde_json::to_string(&beacon_block)?;
    let reparsed: serde_json::Value = serde_json::from_str(&re_encoded)?;
    let original: serde_json::Value = serde_json::from_str(&body)?;

    assert_eq!(
        reparsed, original,
        "Re-encoded block doesn't match original JSON"
    );

    println!("Block Serialization completed successfully");

    Ok(())
}

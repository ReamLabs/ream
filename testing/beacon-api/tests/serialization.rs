use std::{fs, io::Read};

use alloy_primitives::b256;
use ream_consensus::deneb::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState};
use ream_rpc::types::response::BeaconVersionedResponse;
use serde_json::Value;

#[tokio::test]
async fn test_beacon_state_serialization() -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<BeaconState>;

    let file_path = "./tests/assets/state.json";
    let original_json = read_json_file(file_path)?;
    println!("Serialization initiated");

    let beacon_state: Response = serde_json::from_value(original_json.clone())?;

    assert_eq!(beacon_state.version, "deneb");
    assert_eq!(beacon_state.data.latest_block_header.slot, 1);
    assert_eq!(
        beacon_state.data.latest_block_header.parent_root,
        b256!("0xcf8e0d4e9587369b2301d0790347320302cc0943d5a1884560367e8208d920f2")
    );

    let serialized_json: Value = serde_json::to_value(&beacon_state)?;

    assert_eq!(
        original_json, serialized_json,
        "Original JSON and re-serialized JSON do not match!"
    );

    println!("State Serialization completed successfully");

    Ok(())
}
#[tokio::test]
async fn test_beacon_block_serialization() -> anyhow::Result<()> {
    pub type Response = BeaconVersionedResponse<SignedBeaconBlock>;

    let file_path = "./tests/assets/block.json";
    let original_json = read_json_file(file_path)?;

    println!("Block Serialization initiated");
    let beacon_block: Response = serde_json::from_value(original_json.clone())?;

    assert_eq!(beacon_block.version, "deneb");
    assert_eq!(beacon_block.data.message.slot, 11532800);

    let serialized_json: Value = serde_json::to_value(&beacon_block)?;

    assert_eq!(
        serialized_json, original_json,
        "Re-encoded block doesn't match original JSON!"
    );

    println!("Block Serialization completed successfully");

    Ok(())
}

pub fn read_json_file(path: &str) -> anyhow::Result<Value> {
    let mut file = fs::File::open(path)?;
    let mut json_str = String::new();
    file.read_to_string(&mut json_str)?;

    Ok(serde_json::from_str(&json_str)?)
}

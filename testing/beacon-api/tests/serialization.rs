use std::{
    fs,
    path::{Path, PathBuf},
};

use alloy_primitives::b256;
use ream_beacon_api_types::responses::BeaconVersionedResponse;
use ream_consensus::electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState};
use serde_json::Value;

const PATH_TO_TEST_DATA_FOLDER: &str = "./tests/assets";

#[tokio::test]
async fn test_beacon_state_serialization() -> anyhow::Result<()> {
    let original_json = read_json_file("state.json")?;

    let beacon_state: BeaconVersionedResponse<BeaconState> =
        serde_json::from_value(original_json.clone())?;

    assert_eq!(beacon_state.version, "electra");
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

    Ok(())
}

#[tokio::test]
async fn test_beacon_block_serialization() -> anyhow::Result<()> {
    let original_json = read_json_file("block.json")?;

    let beacon_block: BeaconVersionedResponse<SignedBeaconBlock> =
        serde_json::from_value(original_json.clone())?;

    assert_eq!(beacon_block.version, "electra");
    assert_eq!(beacon_block.data.message.slot, 1);

    let serialized_json: Value = serde_json::to_value(&beacon_block)?;

    assert_eq!(
        serialized_json, original_json,
        "Re-encoded block doesn't match original JSON!"
    );

    Ok(())
}

pub fn read_json_file<P: AsRef<Path>>(file_name: P) -> anyhow::Result<Value> {
    let file_contents =
        fs::read_to_string(PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(file_name))?;
    Ok(serde_json::from_str(&file_contents)?)
}

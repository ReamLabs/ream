use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use ream_consensus::{
    constants::SLOTS_PER_EPOCH, electra::beacon_state::BeaconState,
    misc::compute_start_slot_at_epoch,
};
use ream_rpc::{handlers::committee::CommitteeData, types::response::BeaconVersionedResponse};
use serde_json::Value;

const PATH_TO_TEST_DATA_FOLDER: &str = "./tests/assets";

#[test]
fn test_beacon_api_state_committee() -> anyhow::Result<()> {
    let original_json = read_json_file("state.json")?;

    let beacon_state: BeaconVersionedResponse<BeaconState> =
        serde_json::from_value(original_json.clone())?;

    let epoch = beacon_state.data.get_current_epoch();
    let committees_per_slot = beacon_state.data.get_committee_count_per_slot(epoch);

    let slots: Vec<u64> = {
        let start_slot = compute_start_slot_at_epoch(epoch);
        (start_slot..(start_slot + SLOTS_PER_EPOCH)).collect()
    };

    let indices: Vec<u64> = (0..(committees_per_slot * SLOTS_PER_EPOCH)).collect();

    let mut result: Vec<CommitteeData> = Vec::with_capacity(slots.len() * indices.len());

    for slot in &slots {
        for index in &indices {
            let committee = beacon_state
                .data
                .get_beacon_committee(*slot, *index)
                .map_err(|_| {
                    anyhow!(format!(
                        "Sync Committee with slot: {slot} and index: {index} not found"
                    ))
                })?;
            result.push(CommitteeData {
                index: *index,
                slot: *slot,
                validators: committee,
            });
        }
    }

    assert_eq!(result.len(), 1024);
    assert_eq!(result.last().unwrap().slot, 31);

    Ok(())
}

#[test]
fn test_beacon_api_state_committee_at_index() -> anyhow::Result<()> {
    let original_json = read_json_file("state.json")?;

    let beacon_state: BeaconVersionedResponse<BeaconState> =
        serde_json::from_value(original_json.clone())?;

    let epoch = beacon_state.data.get_current_epoch();

    let slots: Vec<u64> = {
        let start_slot = compute_start_slot_at_epoch(epoch);
        (start_slot..(start_slot + SLOTS_PER_EPOCH)).collect()
    };

    let indices: Vec<u64> = vec![3];

    let mut result: Vec<CommitteeData> = Vec::with_capacity(slots.len() * indices.len());

    for slot in &slots {
        for index in &indices {
            let committee = beacon_state
                .data
                .get_beacon_committee(*slot, *index)
                .map_err(|_| {
                    anyhow!(format!(
                        "Sync Committee with slot: {slot} and index: {index} not found"
                    ))
                })?;
            result.push(CommitteeData {
                index: *index,
                slot: *slot,
                validators: committee,
            });
        }
    }

    assert_eq!(result.len(), 32);
    for committee in result.iter() {
        assert_eq!(committee.index, 3);
    }

    Ok(())
}

pub fn read_json_file<P: AsRef<Path>>(file_name: P) -> anyhow::Result<Value> {
    let file_contents =
        fs::read_to_string(PathBuf::from(PATH_TO_TEST_DATA_FOLDER).join(file_name))?;
    Ok(serde_json::from_str(&file_contents)?)
}

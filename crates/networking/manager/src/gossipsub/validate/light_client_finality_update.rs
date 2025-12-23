use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Ok;
use ream_consensus_misc::constants::beacon::SYNC_COMMITTEE_SIZE;
use ream_light_client::finality_update::LightClientFinalityUpdate;
use ream_network_spec::networks::{beacon_network_spec, lean_network_spec};
use ream_storage::cache::BeaconCacheDB;

use crate::gossipsub::validate::result::ValidationResult;

pub async fn validate_light_client_finality_update(
    update: &LightClientFinalityUpdate,
    cached_db: &BeaconCacheDB,
) -> anyhow::Result<ValidationResult> {
    // [IGNORE] The finalized header is greater than that of all previously forwarded finality
    // updates or it matches the highest previously forwarded slot and also has a supermajority
    // participation while previously forwarded slot did not indicate supermajority
    let new_slot = update.finalized_header.beacon.slot;
    let participation_count = update
        .sync_aggregate
        .sync_committee_bits
        .iter()
        .filter(|b| *b)
        .count() as u64;

    let has_supermajority = participation_count * 3 > SYNC_COMMITTEE_SIZE * 2;
    if let Some((prev_slot, prev_slot_supermajority)) =
        *cached_db.seen_forwarded_finality_update_slot.read().await
    {
        if new_slot < prev_slot {
            return Ok(ValidationResult::Ignore(
                "Finality update slot is less than the last forwarded update slot".into(),
            ));
        }

        if new_slot == prev_slot {
            if prev_slot_supermajority {
                return Ok(ValidationResult::Ignore(
                    "Finality update already gossiped".into(),
                ));
            } else if !has_supermajority {
                return Ok(ValidationResult::Ignore(
                    "Worse than previous update".into(),
                ));
            }
        }
    }

    // [IGNORE] The finality_update is received after the block at signature_slot was given enough
    // time to propagate through the network
    let signature_slot_start_time = lean_network_spec().genesis_time
        + (update
            .signature_slot
            .saturating_mul(lean_network_spec().seconds_per_slot));
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Error getting current time")
        .as_secs();

    let due_in_seconds = lean_network_spec().seconds_per_slot.saturating_div(3);

    if current_time
        < signature_slot_start_time + due_in_seconds
            - beacon_network_spec().maximum_gossip_clock_disparity
    {
        return Ok(ValidationResult::Ignore("Too early".to_string()));
    };

    *cached_db.seen_forwarded_finality_update_slot.write().await =
        Some((new_slot, has_supermajority));

    *cached_db
        .forwarded_light_client_finality_update
        .write()
        .await = Some(update.clone());

    Ok(ValidationResult::Accept)
}

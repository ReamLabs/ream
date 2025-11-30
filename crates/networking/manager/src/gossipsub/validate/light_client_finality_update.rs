use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Ok, anyhow};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_misc::constants::beacon::SYNC_COMMITTEE_SIZE;
use ream_light_client::finality_update::LightClientFinalityUpdate;
use ream_network_spec::networks::{beacon_network_spec, lean_network_spec};
use ream_storage::{cache::CachedDB, tables::table::REDBTable};

use crate::gossipsub::validate::result::ValidationResult;

pub async fn validate_light_client_finality_update(
    update: &LightClientFinalityUpdate,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let _state = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get beacon state: {head_root}"))?;

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

    let mut last_forwarded_update = cached_db.seen_forwarded_finality_update_slot.write().await;
    if let Some(ref mut info) = *last_forwarded_update {
        if new_slot < info.0 {
            return Ok(ValidationResult::Ignore(
                "Finality update slot is less than the last forwarded update slot".into(),
            ));
        }

        if new_slot == info.0 {
            if info.1 {
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

    *last_forwarded_update = Some((new_slot, has_supermajority));
    *cached_db
        .forwarded_light_client_finality_update
        .write()
        .await = Some(update.clone());

    Ok(ValidationResult::Accept)
}

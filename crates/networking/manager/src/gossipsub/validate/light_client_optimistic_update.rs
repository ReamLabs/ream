use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_light_client::optimistic_update::LightClientOptimisticUpdate;
use ream_network_spec::networks::{beacon_network_spec, lean_network_spec};
use ream_storage::{cache::CachedDB, tables::table::REDBTable};

use crate::gossipsub::validate::result::ValidationResult;

pub async fn validate_light_client_optimistic_update(
    light_client_optimistic_update: &LightClientOptimisticUpdate,
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

    let signature_slot_start_time = lean_network_spec().genesis_time
        + (light_client_optimistic_update
            .signature_slot
            .saturating_mul(lean_network_spec().seconds_per_slot));
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Error getting current time")
        .as_secs();

    // [IGNORE] The optimistic_update is received after the block at signature_slot was given enough
    // time to propagate through the network
    if current_time
        < signature_slot_start_time - beacon_network_spec().maximum_gossip_clock_disparity
    {
        return Ok(ValidationResult::Ignore("Too early".to_string()));
    };

    let attested_header_slot = light_client_optimistic_update.attested_header.beacon.slot;
    let last_forwarded_slot = *cached_db.forwarded_optimistic_update_slot.read().await;

    // [IGNORE] The attested_header.beacon.slot is greater than that of all previously forwarded
    // optimistic_update(s)
    if last_forwarded_slot.is_some_and(|slot| slot >= attested_header_slot) {
        return Ok(ValidationResult::Ignore(
            "Optimistic update slot is older than previously forwarded update".to_string(),
        ));
    };

    let match_finality_update = if let Some(forwarded_lc_finality_update) = cached_db
        .forwarded_light_client_finality_update
        .read()
        .await
        .clone()
    {
        forwarded_lc_finality_update.attested_header
            == light_client_optimistic_update.attested_header
            && forwarded_lc_finality_update.sync_aggregate
                == light_client_optimistic_update.sync_aggregate
            && forwarded_lc_finality_update.signature_slot
                == light_client_optimistic_update.signature_slot
    } else {
        false
    };

    if !match_finality_update {
        return Ok(ValidationResult::Ignore(
            "Does not match finality update".to_string(),
        ));
    };

    Ok(ValidationResult::Accept)
}

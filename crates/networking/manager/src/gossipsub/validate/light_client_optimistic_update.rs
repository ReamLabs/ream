use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_light_client::optimistic_update::LightClientOptimisticUpdate;
use ream_storage::{cache::CachedDB, tables::table::REDBTable};

use crate::gossipsub::validate::result::ValidationResult;

pub async fn validate_light_client_optimistic_update(
    light_client_optimistic_update: &LightClientOptimisticUpdate,
    beacon_chain: &BeaconChain,
    cache_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let _state = store
        .db
        .state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get beacon state: {head_root}"))?;

    let attested_header_slot = light_client_optimistic_update.attested_header.beacon.slot;
    let last_forwarded_slot = *cache_db.last_forwarded_optimistic_update_slot.read().await;

    // [IGNORE] The attested_header.beacon.slot is greater than that of all previously forwarded
    // optimistic_update(s)
    if last_forwarded_slot.is_some_and(|slot| slot >= attested_header_slot) {
        Ok(ValidationResult::Ignore(
            "Optimistic update slot is older than previously forwarded update".to_string(),
        ))
    } else {
        Ok(ValidationResult::Accept)
    }
}

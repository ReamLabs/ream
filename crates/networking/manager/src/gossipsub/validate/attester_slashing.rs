use std::collections::HashSet;

use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, electra::beacon_state::BeaconState,
};
use ream_storage::{cache::CachedDB, tables::Table};

use super::result::ValidationResult;

pub async fn validate_attester_slashing(
    attester_slashing: &AttesterSlashing,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let mut state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get beacon state: {head_root}"))?;

    let slashed_indices = attester_slashing
        .attestation_1
        .attesting_indices
        .iter()
        .cloned()
        .collect::<HashSet<_>>()
        .intersection(
            &attester_slashing
                .attestation_2
                .attesting_indices
                .iter()
                .cloned()
                .collect::<HashSet<_>>(),
        )
        .cloned()
        .collect::<HashSet<_>>();

    // [IGNORE] At least one index in the intersection of the attesting indices of each attestation
    // has not yet been seen in any prior attester_slashing
    if slashed_indices
        .difference(
            &cached_db
                .prior_seen_attester_slashing_indices
                .read()
                .await
                .iter()
                .map(|(key, _)| *key)
                .collect::<HashSet<_>>(),
        )
        .collect::<HashSet<_>>()
        .is_empty()
    {
        return Ok(ValidationResult::Ignore(
            "All indices have already been seen".to_string(),
        ));
    }

    // [REJECT] All of the conditions within process_attester_slashing pass validation.
    if let Err(err) = state.process_attester_slashing(attester_slashing) {
        return Ok(ValidationResult::Reject(format!(
            "process_attester_slashing fails validation: {err}"
        )));
    }

    for index in slashed_indices {
        cached_db
            .prior_seen_attester_slashing_indices
            .write()
            .await
            .put(index, ());
    }

    Ok(ValidationResult::Accept)
}

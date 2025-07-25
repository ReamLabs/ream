use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_bls::traits::Verifiable;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::{
    constants::DOMAIN_SYNC_COMMITTEE,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use ream_storage::{
    cache::{CachedDB, SyncCommitteeKey},
    tables::Table,
};
use ream_validator_beacon::sync_committee::{
    SyncCommitteeMessage, compute_subnets_for_sync_committee,
};

use super::result::ValidationResult;

pub async fn validate_sync_committee(
    message: &SyncCommitteeMessage,
    beacon_chain: &BeaconChain,
    subnet_id: u64,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    // [IGNORE] The message's slot is for the current slot (with a MAXIMUM_GOSSIP_CLOCK_DISPARITY
    // allowance)
    if message.slot != store.get_current_slot()? {
        return Ok(ValidationResult::Ignore(
            "Message is not from current slot".into(),
        ));
    }

    // [REJECT] The subnet_id is valid for the given validator
    if !compute_subnets_for_sync_committee(&state, message.validator_index)?.contains(&subnet_id) {
        return Ok(ValidationResult::Reject(
            "Validator not in correct sync subcommittee".into(),
        ));
    }

    // [IGNORE] There has been no other valid sync committee message for the declared slot for the
    // validator referenced by sync_committee_message.validator_index (this requires maintaining a
    // cache of size SYNC_COMMITTEE_SIZE // SYNC_COMMITTEE_SUBNET_COUNT for each subnet that can be
    // flushed after each slot). Note this validation is per topic so that for a given slot,
    // multiple messages could be forwarded with the same validator_index as long as the subnet_ids
    // are distinct.
    let key = SyncCommitteeKey {
        subnet_id,
        slot: message.slot,
        validator_index: message.validator_index,
    };
    if cached_db.seen_sync_messages.read().await.contains(&key) {
        return Ok(ValidationResult::Ignore(
            "Duplicate sync committee message".into(),
        ));
    }

    // [REJECT] The signature is valid for the message beacon_block_root for the validator
    // referenced by validator_index.
    let signing_root = compute_signing_root(
        message,
        state.get_domain(
            DOMAIN_SYNC_COMMITTEE,
            Some(compute_epoch_at_slot(message.slot)),
        ),
    );
    if !message.signature.verify(
        &state
            .validators
            .get(message.validator_index as usize)
            .ok_or_else(|| anyhow!("Validator not found"))?
            .public_key,
        signing_root.as_slice(),
    )? {
        return Ok(ValidationResult::Reject(
            "The signature is not valid for the message beacon_block_root for the validator".into(),
        ));
    }

    cached_db.seen_sync_messages.write().await.put(key, ());

    Ok(ValidationResult::Accept)
}

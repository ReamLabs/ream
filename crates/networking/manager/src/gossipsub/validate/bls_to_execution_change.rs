use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    bls_to_execution_change::SignedBLSToExecutionChange, electra::beacon_state::BeaconState,
};
use ream_network_spec::networks::beacon_network_spec;
use ream_storage::{
    cache::{AddressValidaterIndexIdentifier, CachedDB},
    tables::Table,
};

use super::result::ValidationResult;

pub async fn validate_bls_to_execution_change(
    signed: &SignedBLSToExecutionChange,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let mut state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    // [IGNORE] current_epoch >= CAPELLA_FORK_EPOCH, where current_epoch is defined by the current
    // wall-clock time.
    if state.get_current_epoch() < beacon_network_spec().capella_fork_epoch {
        return Ok(ValidationResult::Ignore(
            "Current epoch is before Capella fork".into(),
        ));
    }

    // [IGNORE] The signed_bls_to_execution_change is the first valid signed bls to execution change
    // received for the validator with index
    let key = AddressValidaterIndexIdentifier {
        address: signed.message.from_bls_public_key.clone(),
        validator_index: signed.message.validator_index,
    };

    if cached_db
        .seen_bls_to_execution_change
        .read()
        .await
        .contains(&key)
    {
        return Ok(ValidationResult::Ignore(
            "The signed_bls_to_execution_change is not the first valid signed bls to execution change received for the validator with index".into(),
        ));
    }

    // [REJECT] All of the conditions within process_bls_to_execution_change pass validation.
    if let Err(err) = state.process_bls_to_execution_change(signed) {
        return Ok(ValidationResult::Reject(format!(
            "All of the conditions within process_bls_to_execution_change pass validation fail: {err}"
        )));
    }

    cached_db
        .seen_bls_to_execution_change
        .write()
        .await
        .put(key, ());

    Ok(ValidationResult::Accept)
}

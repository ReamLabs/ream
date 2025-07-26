use anyhow::anyhow;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    electra::beacon_state::BeaconState, voluntary_exit::SignedVoluntaryExit,
};
use ream_storage::{cache::CachedDB, tables::Table};

use super::result::ValidationResult;

pub async fn validate_voluntary_exit(
    voluntary_exit: &SignedVoluntaryExit,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let store = beacon_chain.store.lock().await;

    let head_root = store.get_head()?;
    let state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("No beacon state found for head root: {head_root}"))?;

    // [IGNORE] The voluntary exit is the first valid voluntary exit received for the validator with
    // index signed_voluntary_exit.message.validator_index
    if cached_db
        .seen_voluntary_exit
        .read()
        .await
        .contains(&voluntary_exit.message.validator_index)
    {
        let index = voluntary_exit.message.validator_index;
        return Ok(ValidationResult::Ignore(format!(
            "The voluntary_exit is not the first valid voluntary exit received for the validator with index: {index}"
        )));
    }

    // [REJECT] All of the conditions within process_voluntary_exit pass validation.
    if let Err(err) = state.validate_voluntary_exit(voluntary_exit) {
        return Ok(ValidationResult::Reject(format!(
            "All of the conditions within validate_voluntary_exit pass validation fail: {err}"
        )));
    }

    cached_db
        .seen_voluntary_exit
        .write()
        .await
        .put(voluntary_exit.message.validator_index, ());

    Ok(ValidationResult::Accept)
}

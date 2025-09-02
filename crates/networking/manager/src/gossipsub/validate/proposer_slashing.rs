use anyhow::anyhow;
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    electra::beacon_state::BeaconState, proposer_slashing::ProposerSlashing,
};
use ream_storage::{cache::CachedDB, tables::table::Table};

use super::result::ValidationResult;

pub async fn validate_proposer_slashing(
    proposer_slashing: &ProposerSlashing,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
) -> anyhow::Result<ValidationResult> {
    let proposer_index = proposer_slashing.signed_header_1.message.proposer_index;

    // [IGNORE] The proposer slashing is the first valid proposer slashing received for the proposer
    // with index proposer_slashing.signed_header_1.message.proposer_index
    if cached_db
        .seen_proposer_slashings
        .read()
        .await
        .contains(&proposer_index)
    {
        return Ok(ValidationResult::Ignore(
            "The proposer slashing is not the first valid".into(),
        ));
    }

    let store = beacon_chain.store.lock().await;
    let head_root = store.get_head()?;
    let mut state: BeaconState = store
        .db
        .beacon_state_provider()
        .get(head_root)?
        .ok_or_else(|| anyhow!("Could not get beacon state: {head_root}"))?;

    // [REJECT] All of the conditions within process_proposer_slashing pass validation
    if let Err(err) = state.process_proposer_slashing(proposer_slashing) {
        return Ok(ValidationResult::Reject(format!(
            "Not all of the conditions within process_proposer_slashing pass validation: {err}"
        )));
    }

    cached_db
        .seen_proposer_slashings
        .write()
        .await
        .put(proposer_index, ());

    Ok(ValidationResult::Accept)
}

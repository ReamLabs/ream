use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_storage::cache::CachedDB;
use ream_validator_beacon::contribution_and_proof::SignedContributionAndProof;

use super::result::ValidationResult;

pub async fn validate_sync_committee_contribution_and_proof(
    _beacon_chain: &BeaconChain,
    _cached_db: &CachedDB,
    _contribution_and_proof: &SignedContributionAndProof,
) -> anyhow::Result<ValidationResult> {
    Ok(ValidationResult::Accept)
}

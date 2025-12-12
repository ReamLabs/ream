use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};

use crate::contribution_and_proof::ContributionAndProof;

/// Contribution and proof event.
///
/// The node has received a SignedContributionAndProof (from P2P or API) that passes
/// validation rules of the `sync_committee_contribution_and_proof` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAndProofEvent {
    pub message: ContributionAndProof,
    pub signature: BLSSignature,
}

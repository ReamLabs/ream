use alloy_primitives::B256;
use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus::misc::{compute_domain, compute_signing_root};
use ream_network_spec::networks::network_spec;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U128};
use tree_hash_derive::TreeHash;

use crate::{
    constants::DOMAIN_CONTRIBUTION_AND_PROOF, sync_committee::get_sync_committee_selection_proof,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommitteeContribution {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub beacon_block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub subcommittee_index: u64,
    pub aggregation_bits: BitVector<U128>,
    pub signature: BLSSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ContributionAndProof {
    #[serde(with = "serde_utils::quoted_u64")]
    pub aggregator_index: u64,
    pub contribution: SyncCommitteeContribution,
    pub selection_proof: BLSSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedContributionAndProof {
    pub message: ContributionAndProof,
    pub signature: BLSSignature,
}

pub fn get_contribution_and_proof(
    contribution: SyncCommitteeContribution,
    aggregator_index: u64,
    private_key: &PrivateKey,
) -> anyhow::Result<ContributionAndProof> {
    Ok(ContributionAndProof {
        selection_proof: get_sync_committee_selection_proof(
            contribution.slot,
            contribution.subcommittee_index,
            private_key,
        )?,
        aggregator_index,
        contribution,
    })
}

pub fn get_contribution_and_proof_signature(
    contribution_and_proof: ContributionAndProof,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = compute_domain(
        DOMAIN_CONTRIBUTION_AND_PROOF,
        Some(network_spec().electra_fork_version),
        None,
    );
    let signing_root = compute_signing_root(contribution_and_proof, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}

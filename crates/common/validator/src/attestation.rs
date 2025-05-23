use ream_bls::{BLSSignature, SecretKey};
use ream_consensus::{
    attestation_data::AttestationData,
    constants::{DOMAIN_BEACON_ATTESTER, SLOTS_PER_EPOCH},
    electra::beacon_state::BeaconState,
    misc::compute_signing_root,
};

use crate::constants::ATTESTATION_SUBNET_COUNT;

/// Compute the correct subnet for an attestation for Phase 0.
/// Note, this mimics expected future behavior where attestations will be mapped to their shard
/// subnet.
pub fn compute_subnet_for_attestation(
    committees_per_slot: u64,
    slot: u64,
    committee_index: u64,
) -> u64 {
    let slots_since_epoch_start = slot % SLOTS_PER_EPOCH;
    let committee_since_epoch_start = committees_per_slot * slots_since_epoch_start;
    (committee_since_epoch_start + committee_index) % ATTESTATION_SUBNET_COUNT
}

pub fn get_attestation_signature(
    state: &BeaconState,
    attestation_data: AttestationData,
    private_key: u32,
) -> BLSSignature {
    let domain = state.get_domain(DOMAIN_BEACON_ATTESTER, Some(attestation_data.target.epoch));
    let signing_root = compute_signing_root(attestation_data, domain);
    let key_bytes = private_key.to_le_bytes();
    let secret_key = SecretKey::from_bytes(&key_bytes).expect("Invalid private key");

    // Sign the message and return the signature
    secret_key.sign(&signing_root)
}

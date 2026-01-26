use ream_post_quantum_crypto::leansig::public_key::PublicKey;

use crate::validator::Validator;

/// Macro to log skipped attestations with feature-specific identifier
#[macro_export]
macro_rules! log_skip_attestation {
    ($reason:expr, $attestation:expr) => {
        #[cfg(feature = "devnet1")]
        info!(
            reason = $reason,
            source_slot = $attestation.source().slot,
            target_slot = $attestation.target().slot,
            "Skipping attestation by Validator {}",
            $attestation.validator_id,
        );
        #[cfg(feature = "devnet2")]
        info!(
            reason = $reason,
            source_slot = $attestation.source().slot,
            target_slot = $attestation.target().slot,
            "Skipping attestation: {:?}",
            $attestation.aggregation_bits,
        );
    };
}

pub fn generate_default_validators(number_of_validators: usize) -> Vec<Validator> {
    (0..number_of_validators)
        .map(|index| Validator {
            public_key: PublicKey::from(&[0_u8; 52][..]),
            index: index as u64,
        })
        .collect()
}

#[cfg(feature = "devnet2")]
pub fn justified_index_after(candidate_slot: u64, finalized_slot: u64) -> Option<u64> {
    if candidate_slot <= finalized_slot {
        return None;
    }

    Some(candidate_slot - finalized_slot - 1)
}

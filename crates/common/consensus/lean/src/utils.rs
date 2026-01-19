use ream_post_quantum_crypto::leansig::public_key::PublicKey;

use crate::validator::Validator;

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

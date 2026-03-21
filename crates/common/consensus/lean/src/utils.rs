use ream_post_quantum_crypto::leansig::public_key::PublicKey;

use crate::validator::Validator;

pub fn generate_default_validators(number_of_validators: usize) -> Vec<Validator> {
    (0..number_of_validators)
        .map(|index| Validator::from_public_key(PublicKey::from(&[0_u8; 52][..]), index as u64))
        .collect()
}

use hashsig::signature::SignatureScheme;
use rand::Rng;

use super::private_key::HashSigScheme;
use crate::hashsig::{PrivateKey, PublicKey};

pub fn generate<R: Rng>(
    rng: &mut R,
    activation_epoch: usize,
    num_active_epochs: usize,
) -> (PublicKey, PrivateKey) {
    let (public_key, private_key) =
        <HashSigScheme as SignatureScheme>::key_gen(rng, activation_epoch, num_active_epochs);

    (PublicKey::new(public_key), PrivateKey::new(private_key))
}

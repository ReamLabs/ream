use hashsig::signature::SignatureScheme;

use crate::hashsig::private_key::HashSigScheme;
type HashSigPublicKey = <HashSigScheme as SignatureScheme>::PublicKey;

pub struct PublicKey {
    pub inner: HashSigPublicKey,
}

impl PublicKey {
    pub fn new(inner: HashSigPublicKey) -> Self {
        Self { inner }
    }
}

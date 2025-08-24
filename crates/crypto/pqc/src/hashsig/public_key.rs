use hashsig::signature::SignatureScheme;

use crate::hashsig::HashSigScheme;

type HashSigPublicKey = <HashSigScheme as SignatureScheme>::PublicKey;

pub struct PublicKey {
    pub inner: HashSigPublicKey,
}

impl PublicKey {
    pub fn new(inner: HashSigPublicKey) -> Self {
        Self { inner }
    }
}

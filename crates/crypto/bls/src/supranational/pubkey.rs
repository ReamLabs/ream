use blst::min_pk::PublicKey as BlstPublicKey;
use ssz_types::FixedVector;

use crate::{errors::BLSError, pubkey::PubKey};

impl From<BlstPublicKey> for PubKey {
    fn from(value: BlstPublicKey) -> Self {
        PubKey {
            inner: FixedVector::from(value.to_bytes().to_vec()),
        }
    }
}

impl PubKey {
    pub fn to_blst_pubkey(&self) -> Result<BlstPublicKey, BLSError> {
        BlstPublicKey::from_bytes(&self.inner).map_err(|err| BLSError::BlstError(err.into()))
    }
}

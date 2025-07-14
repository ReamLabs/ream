use alloy_primitives::Address;
use ream_bls::{BLSSignature, PrivateKey, PublicKey, traits::Signable};
use ream_consensus_misc::misc::{compute_domain, compute_signing_root};
use serde::{Deserialize, Serialize};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::DOMAIN_APPLICATION_BUILDER;

#[derive(Debug, PartialEq, Eq, Clone, TreeHash, Serialize, Deserialize)]
pub struct ValidatorRegistrationV1 {
    pub fee_recipient: Address,
    #[serde(with = "serde_utils::quoted_u64")]
    pub gas_limit: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub timestamp: u64,
    pub public_key: PublicKey,
}

impl ValidatorRegistrationV1 {
    pub fn create_signed_registration(
        &self,
        private_key: &PrivateKey,
    ) -> anyhow::Result<SignedValidatorRegistrationV1> {
        let domain = compute_domain(DOMAIN_APPLICATION_BUILDER, None, None);
        let signature = compute_signing_root(self.tree_hash_root(), domain);
        Ok(SignedValidatorRegistrationV1 {
            message: self.clone(),
            signature: private_key.sign(signature.as_ref())?,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SignedValidatorRegistrationV1 {
    pub message: ValidatorRegistrationV1,
    pub signature: BLSSignature,
}

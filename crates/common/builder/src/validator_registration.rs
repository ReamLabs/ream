use alloy_primitives::Address;
use ream_bls::{BLSSignature, PubKey};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, TreeHash)]
pub struct ValidatorRegistrationV1 {
    pub fee_recipient: Address,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub pubkey: PubKey,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SignedValidatorRegistrationV1 {
    pub message: ValidatorRegistrationV1,
    pub signature: BLSSignature,
}

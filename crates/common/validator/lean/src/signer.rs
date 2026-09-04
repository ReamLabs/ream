use std::sync::Arc;

use alloy_primitives::B256;
use ream_keystore::lean_keystore::ValidatorKeystore;
use ream_post_quantum_crypto::leansig::{
    errors::LeanSigError, public_key::PublicKey, signature::Signature,
};

/// Result of signing a Lean block proposal root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposalSignature {
    pub public_key: PublicKey,
    pub signature: Signature,
    pub epoch: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProposalSignerError {
    #[error("proposal key for validator {validator_index} was not found")]
    ValidatorNotFound { validator_index: u64 },

    #[error("slot {slot} cannot be represented as an XMSS epoch")]
    SlotOverflow { slot: u64 },

    #[error(
        "slot {slot} is outside validator {validator_index}'s proposal-key activation interval {activation_start}..{activation_end}"
    )]
    SlotOutsideActivationInterval {
        validator_index: u64,
        slot: u64,
        activation_start: u64,
        activation_end: u64,
    },

    #[error("failed to sign proposal for validator {validator_index} at slot {slot}: {source}")]
    SigningFailed {
        validator_index: u64,
        slot: u64,
        #[source]
        source: LeanSigError,
    },
}

impl ProposalSignerError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ValidatorNotFound { .. } => "validator_not_found",
            Self::SlotOverflow { .. } => "slot_overflow",
            Self::SlotOutsideActivationInterval { .. } => "slot_outside_activation_interval",
            Self::SigningFailed { .. } => "signing_failed",
        }
    }
}

/// Shared proposal-signing boundary used by production block building and test drivers.
pub struct ProposalSigner {
    keystores: Vec<Arc<ValidatorKeystore>>,
}

impl ProposalSigner {
    pub fn new(keystores: Vec<Arc<ValidatorKeystore>>) -> Self {
        Self { keystores }
    }

    pub fn first_validator_index(&self) -> Option<u64> {
        self.keystores.first().map(|keystore| keystore.index)
    }

    pub fn validator_indices(&self) -> impl Iterator<Item = u64> + '_ {
        self.keystores.iter().map(|keystore| keystore.index)
    }

    pub(crate) fn keystores(&self) -> &[Arc<ValidatorKeystore>] {
        &self.keystores
    }

    pub(crate) fn has_validator(&self, validator_index: u64) -> bool {
        self.keystore(validator_index).is_some()
    }

    /// Sign the supplied block root with a validator's proposal key.
    pub fn sign_proposal(
        &self,
        validator_index: u64,
        slot: u64,
        block_root: &B256,
    ) -> Result<ProposalSignature, ProposalSignerError> {
        let keystore = self
            .keystore(validator_index)
            .ok_or(ProposalSignerError::ValidatorNotFound { validator_index })?;
        let epoch = u32::try_from(slot).or(Err(ProposalSignerError::SlotOverflow { slot }))?;
        let activation_interval = keystore.proposal_private_key.get_activation_interval();
        if !activation_interval.contains(&slot) {
            return Err(ProposalSignerError::SlotOutsideActivationInterval {
                validator_index,
                slot,
                activation_start: activation_interval.start,
                activation_end: activation_interval.end,
            });
        }
        let signature = keystore
            .proposal_private_key
            .sign(block_root, epoch)
            .map_err(|err| ProposalSignerError::SigningFailed {
                validator_index,
                slot,
                source: err,
            })?;

        Ok(ProposalSignature {
            public_key: keystore.proposal_public_key,
            signature,
            epoch,
        })
    }

    fn keystore(&self, validator_index: u64) -> Option<&ValidatorKeystore> {
        self.keystores
            .iter()
            .find(|keystore| keystore.index == validator_index)
            .map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alloy_primitives::B256;
    use ream_keystore::lean_keystore::ValidatorKeystore;
    use ream_post_quantum_crypto::leansig::private_key::PrivateKey;

    use super::{ProposalSigner, ProposalSignerError};

    fn test_signer() -> ProposalSigner {
        let (attestation_public_key, attestation_private_key) =
            PrivateKey::generate_key_pair_from_seed([1; 32], 0, 4);
        let (proposal_public_key, proposal_private_key) =
            PrivateKey::generate_key_pair_from_seed([2; 32], 0, 4);

        ProposalSigner::new(vec![Arc::new(ValidatorKeystore {
            index: 7,
            attestation_public_key,
            proposal_public_key,
            attestation_private_key,
            proposal_private_key,
        })])
    }

    #[test]
    fn signs_with_the_requested_proposal_key() {
        let signer = test_signer();
        let block_root = B256::repeat_byte(0x42);

        let signed = signer
            .sign_proposal(7, 2, &block_root)
            .expect("proposal signing should succeed");

        assert_eq!(signed.public_key, signer.keystores()[0].proposal_public_key);
        assert_eq!(signed.epoch, 2);
        assert!(
            signed
                .signature
                .verify(&signed.public_key, 2, block_root.as_ref())
                .expect("signature verification should run")
        );
    }

    #[test]
    fn rejects_an_unknown_validator() {
        let error = test_signer()
            .sign_proposal(8, 2, &B256::ZERO)
            .expect_err("unknown validator should be rejected");

        assert!(matches!(
            error,
            ProposalSignerError::ValidatorNotFound { validator_index: 8 }
        ));
        assert_eq!(error.code(), "validator_not_found");
    }

    #[test]
    fn rejects_a_slot_that_cannot_be_an_xmss_epoch() {
        let slot = u64::from(u32::MAX) + 1;
        let error = test_signer()
            .sign_proposal(7, slot, &B256::ZERO)
            .expect_err("oversized slot should be rejected");

        assert!(matches!(
            error,
            ProposalSignerError::SlotOverflow { slot: rejected } if rejected == slot
        ));
        assert_eq!(error.code(), "slot_overflow");
    }

    #[test]
    fn rejects_a_slot_outside_the_key_activation_interval() {
        let signer = test_signer();
        let activation_interval = signer.keystores()[0]
            .proposal_private_key
            .get_activation_interval();
        let slot = activation_interval.end;
        let error = signer
            .sign_proposal(7, slot, &B256::ZERO)
            .expect_err("inactive slot should be rejected");

        assert!(matches!(
            error,
            ProposalSignerError::SlotOutsideActivationInterval {
                validator_index: 7,
                slot: rejected_slot,
                activation_start: 0,
                activation_end,
            } if rejected_slot == slot && activation_end == slot
        ));
        assert_eq!(error.code(), "slot_outside_activation_interval");
    }
}

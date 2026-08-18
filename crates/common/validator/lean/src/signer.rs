use std::sync::Arc;

use alloy_primitives::B256;
use anyhow::anyhow;
use ream_keystore::lean_keystore::ValidatorKeystore;
use ream_post_quantum_crypto::leansig::{public_key::PublicKey, signature::Signature};

/// Result of signing a Lean block proposal root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposalSignature {
    pub public_key: PublicKey,
    pub signature: Signature,
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
    ) -> anyhow::Result<ProposalSignature> {
        let keystore = self
            .keystore(validator_index)
            .ok_or_else(|| anyhow!("proposal key for validator {validator_index} was not found"))?;
        let signature = keystore
            .proposal_private_key
            .sign(block_root, slot as u32)?;

        Ok(ProposalSignature {
            public_key: keystore.proposal_public_key,
            signature,
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

    use super::ProposalSigner;

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
        assert!(
            signed
                .signature
                .verify(&signed.public_key, 2, block_root.as_ref())
                .expect("signature verification should run")
        );
    }
}

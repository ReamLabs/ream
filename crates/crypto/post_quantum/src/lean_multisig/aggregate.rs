use anyhow::anyhow;
use bincode::{deserialize, serialize};
use lean_multisig::{
    Devnet2XmssAggregateSignature, xmss_aggregate_signatures, xmss_aggregation_setup_prover,
    xmss_aggregation_setup_verifier, xmss_verify_aggregated_signatures,
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

use crate::leansig::{public_key::PublicKey, signature::Signature};

/// Aggregated signature for multiple leansig signatures
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct AggregateSignature {
    /// Serialized proof bytes
    pub proof_bytes: Vec<u8>,
    /// Encoding randomness for each signature
    pub encoding_randomness: Vec<Vec<u8>>,
}

impl AggregateSignature {
    pub fn new(proof_bytes: Vec<u8>, encoding_randomness: Vec<Vec<u8>>) -> Self {
        Self {
            proof_bytes,
            encoding_randomness,
        }
    }

    /// Convert from the lean-multisig aggregate signature type
    pub fn from_lean_multisig(
        aggregate_signature: Devnet2XmssAggregateSignature,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            proof_bytes: aggregate_signature.proof_bytes,
            encoding_randomness: aggregate_signature
                .encoding_randomness
                .iter()
                .map(|randomness_field_element| {
                    // Serialize the field elements to bytes
                    // Each field element is 32 bits (4 bytes) for KoalaBear
                    serialize(randomness_field_element)
                        .map_err(|err| anyhow!("Failed to serialize encoding randomness: {err}"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    /// Convert to the lean-multisig aggregate signature type
    pub fn to_lean_multisig(&self) -> anyhow::Result<Devnet2XmssAggregateSignature> {
        Ok(Devnet2XmssAggregateSignature {
            proof_bytes: self.proof_bytes.clone(),
            encoding_randomness: self
                .encoding_randomness
                .iter()
                .map(|bytes| {
                    deserialize(bytes)
                        .map_err(|err| anyhow!("Failed to deserialize encoding randomness: {err}"))
                })
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Setup function for the prover side of XMSS aggregation
pub fn setup_prover() {
    xmss_aggregation_setup_prover();
}

/// Setup function for the verifier side of XMSS aggregation
pub fn setup_verifier() {
    xmss_aggregation_setup_verifier();
}

/// Aggregate multiple leansig signatures into a single proof
pub fn aggregate_signatures(
    public_keys: &[PublicKey],
    signatures: &[Signature],
    message: &[u8; 32],
    epoch: u32,
) -> anyhow::Result<AggregateSignature> {
    if public_keys.len() != signatures.len() {
        return Err(anyhow!(
            "Public key count ({}) does not match signature count ({})",
            public_keys.len(),
            signatures.len()
        ));
    }

    AggregateSignature::from_lean_multisig(
        xmss_aggregate_signatures(
            &public_keys
                .iter()
                .map(|public_key| public_key.as_lean_sig())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| anyhow!("Failed to convert public keys: {err}"))?,
            &signatures
                .iter()
                .map(|signature| signature.as_lean_sig())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|err| anyhow!("Failed to convert signatures: {err}"))?,
            message,
            epoch,
        )
        .map_err(|err| anyhow!("Failed to aggregate signatures: {err:?}"))?,
    )
}

/// Verify an aggregated signature
pub fn verify_aggregate_signature(
    public_keys: &[PublicKey],
    message: &[u8; 32],
    aggregate_signature: &AggregateSignature,
    epoch: u32,
) -> anyhow::Result<()> {
    // Verify using lean-multisig
    xmss_verify_aggregated_signatures(
        &public_keys
            .iter()
            .map(|public_key| public_key.as_lean_sig())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!("Failed to convert public keys: {err}"))?,
        message,
        &aggregate_signature.to_lean_multisig()?,
        epoch,
    )
    .map_err(|err| anyhow!("Failed to verify aggregated signatures: {err}"))
}

pub fn verify_aggregate_signature_bytes(
    public_keys: &[PublicKey],
    message: &[u8; 32],
    proof_bytes: &[u8],
    epoch: u32,
) -> anyhow::Result<()> {
    xmss_verify_aggregated_signatures(
        &public_keys
            .iter()
            .map(|public_key| public_key.as_lean_sig())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!("Failed to convert public keys: {err}"))?,
        message,
        &Devnet2XmssAggregateSignature {
            proof_bytes: proof_bytes.to_vec(),
            encoding_randomness: vec![],
        },
        epoch,
    )
    .map_err(|err| anyhow!("Failed to verify aggregated signatures: {err}"))
}

#[cfg(test)]
mod tests {
    use rand::rng;

    use crate::{
        lean_multisig::aggregate::{
            aggregate_signatures, setup_prover, setup_verifier, verify_aggregate_signature,
        },
        leansig::private_key::PrivateKey,
    };

    #[test]
    fn test_aggregate_and_verify() {
        setup_prover();
        setup_verifier();

        let mut rng = rng();
        let message = [42u8; 32];
        let epoch = 50u32;

        // Generate 3 key pairs with different lifetimes
        let key_configs = vec![(10, 32), (20, 64), (30, 128)];

        let mut public_keys = Vec::new();
        let mut signatures = Vec::new();

        for (activation_epoch, num_active_epochs) in key_configs {
            let (pub_key, mut priv_key) =
                PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

            // Advance the private key preparation to the signing epoch
            let mut iterations = 0;
            while !priv_key.get_prepared_interval().contains(&(epoch as u64))
                && iterations < (epoch - activation_epoch as u32)
            {
                priv_key.prepare_signature();
                iterations += 1;
            }

            let signature = priv_key.sign(&message, epoch).unwrap();

            // Verify individual signature
            assert!(signature.verify(&pub_key, epoch, &message).unwrap());

            public_keys.push(pub_key);
            signatures.push(signature);
        }

        // Aggregate signatures
        let aggregate_signature =
            aggregate_signatures(&public_keys, &signatures, &message, epoch).unwrap();

        // Verify aggregate signature
        verify_aggregate_signature(&public_keys, &message, &aggregate_signature, epoch).unwrap();
    }
}

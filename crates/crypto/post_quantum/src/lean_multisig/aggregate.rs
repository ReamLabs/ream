use anyhow::anyhow;
use lean_multisig::{
    Devnet2XmssAggregateSignature, xmss_aggregate_signatures, xmss_aggregation_setup_prover,
    xmss_aggregation_setup_verifier, xmss_verify_aggregated_signatures,
};
use ssz::{Decode, Encode};

use crate::leansig::{public_key::PublicKey, signature::Signature};

/// Setup function for the prover side of XMSS aggregation
pub fn setup_prover() {
    xmss_aggregation_setup_prover();
}

/// Setup function for the verifier side of XMSS aggregation
pub fn setup_verifier() {
    xmss_aggregation_setup_verifier();
}

/// Aggregate multiple leansig signatures into a single proof.
pub fn aggregate_signatures(
    public_keys: &[PublicKey],
    signatures: &[Signature],
    message: &[u8; 32],
    epoch: u32,
) -> anyhow::Result<Vec<u8>> {
    if public_keys.len() != signatures.len() {
        return Err(anyhow!(
            "Public key count ({}) does not match signature count ({})",
            public_keys.len(),
            signatures.len()
        ));
    }

    let aggregate_signature = xmss_aggregate_signatures(
        &public_keys
            .iter()
            .map(|public_key| public_key.as_lean_sig())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!("Failed to convert public keys: {err}"))?,
        &signatures
            .iter()
            .map(|signature| signature.as_lean_sig().clone())
            .collect::<Vec<_>>(),
        message,
        epoch,
    )
    .map_err(|err| anyhow!("Failed to aggregate signatures: {err:?}"))?;

    Ok(aggregate_signature.as_ssz_bytes())
}

/// Verify an aggregated signature from SSZ-encoded bytes.
pub fn verify_aggregate_signature(
    public_keys: &[PublicKey],
    message: &[u8; 32],
    aggregate_signature_bytes: &[u8],
    epoch: u32,
) -> anyhow::Result<()> {
    let aggregate_signature =
        Devnet2XmssAggregateSignature::from_ssz_bytes(aggregate_signature_bytes)
            .map_err(|err| anyhow!("Failed to decode aggregate signature: {err:?}"))?;

    xmss_verify_aggregated_signatures(
        &public_keys
            .iter()
            .map(|public_key| public_key.as_lean_sig())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow!("Failed to convert public keys: {err}"))?,
        message,
        &aggregate_signature,
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

        let key_configs = vec![(10, 32), (20, 64), (30, 128)];

        let mut public_keys = Vec::new();
        let mut signatures = Vec::new();

        for (activation_epoch, num_active_epochs) in key_configs {
            let (pub_key, mut priv_key) =
                PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

            let mut iterations = 0;
            while !priv_key.get_prepared_interval().contains(&(epoch as u64))
                && iterations < (epoch - activation_epoch as u32)
            {
                priv_key.prepare_signature();
                iterations += 1;
            }

            let signature = priv_key.sign(&message, epoch).unwrap();
            assert!(signature.verify(&pub_key, epoch, &message).unwrap());

            public_keys.push(pub_key);
            signatures.push(signature);
        }

        let aggregate_signature_bytes =
            aggregate_signatures(&public_keys, &signatures, &message, epoch).unwrap();

        verify_aggregate_signature(&public_keys, &message, &aggregate_signature_bytes, epoch)
            .unwrap();
    }
}

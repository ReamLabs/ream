use anyhow::anyhow;
use lean_multisig::{
    AggregatedXMSS, setup_prover, setup_verifier, xmss_aggregate, xmss_verify_aggregation,
};

use crate::leansig::{public_key::PublicKey, signature::Signature};

/// Setup function for the prover side of XMSS aggregation.
pub fn aggregation_setup_prover() {
    setup_prover();
}

/// Setup function for the verifier side of XMSS aggregation.
pub fn aggregation_setup_verifier() {
    setup_verifier();
}

/// Default log inverse rate for WHIR (1/4 rate).
const DEFAULT_LOG_INV_RATE: usize = 2;

/// Aggregate raw XMSS signatures into a single proof.
///
/// Returns serialized `AggregatedXMSS` proof bytes.
pub fn aggregate_signatures(
    public_keys: &[PublicKey],
    signatures: &[Signature],
    message: &[u8; 32],
    slot: u32,
) -> anyhow::Result<Vec<u8>> {
    aggregate_signatures_recursive(&[], public_keys, signatures, message, slot)
}

/// Represents a child proof for recursive aggregation — an already-aggregated proof
/// with its associated public keys.
pub struct ChildProof {
    pub public_keys: Vec<PublicKey>,
    pub proof_data: Vec<u8>,
}

/// Aggregate raw XMSS signatures with recursive child proofs.
///
/// Returns serialized `AggregatedXMSS` proof bytes (includes bytecode_point for recursion).
pub fn aggregate_signatures_recursive(
    children: &[ChildProof],
    public_keys: &[PublicKey],
    signatures: &[Signature],
    message: &[u8; 32],
    slot: u32,
) -> anyhow::Result<Vec<u8>> {
    if public_keys.len() != signatures.len() {
        return Err(anyhow!(
            "Public key count ({}) does not match signature count ({})",
            public_keys.len(),
            signatures.len()
        ));
    }

    // Convert raw XMSS key-signature pairs
    let raw_xmss: Vec<_> = public_keys
        .iter()
        .zip(signatures.iter())
        .map(|(pk, sig)| {
            Ok((
                pk.as_lean_sig()
                    .map_err(|err| anyhow!("Failed to convert public key: {err}"))?,
                sig.as_lean_sig()
                    .map_err(|err| anyhow!("Failed to convert signature: {err}"))?,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Convert child proofs: deserialize proof_data back to AggregatedXMSS + convert pubkeys
    let child_data: Vec<(Vec<_>, AggregatedXMSS)> = children
        .iter()
        .map(|child| {
            let pubkeys = child
                .public_keys
                .iter()
                .map(|pk| {
                    pk.as_lean_sig()
                        .map_err(|err| anyhow!("Failed to convert child public key: {err}"))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let aggregated = AggregatedXMSS::deserialize(&child.proof_data)
                .ok_or_else(|| anyhow!("Failed to deserialize child AggregatedXMSS proof"))?;
            Ok((pubkeys, aggregated))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let child_refs: Vec<(&[_], AggregatedXMSS)> = child_data
        .iter()
        .map(|(pks, agg)| (pks.as_slice(), agg.clone()))
        .collect();

    let (_global_pubkeys, aggregated) =
        xmss_aggregate(&child_refs, raw_xmss, message, slot, DEFAULT_LOG_INV_RATE);

    Ok(aggregated.serialize())
}

/// Verify an aggregated signature proof.
pub fn verify_aggregate_signature(
    public_keys: &[PublicKey],
    message: &[u8; 32],
    proof_data: &[u8],
    slot: u32,
) -> anyhow::Result<()> {
    let aggregated = AggregatedXMSS::deserialize(proof_data)
        .ok_or_else(|| anyhow!("Failed to deserialize AggregatedXMSS proof"))?;

    let lean_pubkeys: Vec<_> = public_keys
        .iter()
        .map(|pk| {
            pk.as_lean_sig()
                .map_err(|err| anyhow!("Failed to convert public key: {err}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    xmss_verify_aggregation(lean_pubkeys, &aggregated, message, slot)
        .map_err(|err| anyhow!("Aggregated signature verification failed: {err:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        lean_multisig::aggregate::{
            aggregate_signatures, aggregation_setup_prover, aggregation_setup_verifier,
            verify_aggregate_signature,
        },
        leansig::private_key::PrivateKey,
    };

    #[test]
    fn test_aggregate_and_verify() {
        aggregation_setup_prover();
        aggregation_setup_verifier();

        let message = [42u8; 32];
        let epoch = 50u32;

        let key_configs = vec![(10, 32), (20, 64), (30, 128)];

        let mut public_keys = Vec::new();
        let mut signatures = Vec::new();

        for (activation_epoch, num_active_epochs) in key_configs {
            let (pub_key, mut priv_key) =
                PrivateKey::generate_key_pair(activation_epoch, num_active_epochs);

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

        let proof_data =
            aggregate_signatures(&public_keys, &signatures, &message, epoch).unwrap();

        verify_aggregate_signature(&public_keys, &message, &proof_data, epoch).unwrap();
    }
}

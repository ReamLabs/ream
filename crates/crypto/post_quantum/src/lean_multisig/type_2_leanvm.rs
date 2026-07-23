use anyhow::{Result, anyhow};
use lean_multisig_leanvm::{
    MultiMessageAggregateSignature as MultiMessageAggregate,
    SingleMessageAggregateSignature as SingleMessageAggregate, XmssPublicKey, XmssSignature,
    aggregate_single_message_signatures, merge_single_message_aggregates, setup_prover,
    setup_verifier, split_multi_message_aggregate, verify_multi_message_aggregate,
    verify_single_message_aggregate,
};

use crate::leansig::{public_key::PublicKey, signature::Signature};

pub const LOG_INV_RATE: usize = 2;

pub fn type_2_setup() {
    setup_prover();
}

pub fn type_2_setup_verifier() {
    setup_verifier();
}

fn to_lib_public_key(public_key: &PublicKey) -> Result<XmssPublicKey> {
    public_key.as_lean_sig()
}

fn to_lib_signature(signature: &Signature) -> Result<XmssSignature> {
    signature.as_lean_sig()
}

fn ensure_expected_pubkeys(embedded: &[XmssPublicKey], expected: &[PublicKey]) -> Result<()> {
    let mut expected = expected
        .iter()
        .map(to_lib_public_key)
        .collect::<Result<Vec<_>>>()?;
    expected.sort();
    expected.dedup();
    if embedded != expected.as_slice() {
        return Err(anyhow!(
            "Aggregate proof public keys do not match the expected validator keys"
        ));
    }
    Ok(())
}

pub fn type_1_from_wire(wire: &[u8], public_keys: &[PublicKey]) -> Result<SingleMessageAggregate> {
    type_2_setup_verifier();
    let proof = SingleMessageAggregate::from_bytes(wire).ok_or_else(|| {
        anyhow!("Failed to decode single-message aggregate multi-signature from wire bytes")
    })?;
    ensure_expected_pubkeys(&proof.info.pubkeys, public_keys)?;
    Ok(proof)
}

pub fn type_1_to_wire(proof: &SingleMessageAggregate) -> Vec<u8> {
    proof.to_bytes()
}

pub fn type_1_aggregate(
    children: &[SingleMessageAggregate],
    raw_xmss: &[(PublicKey, Signature)],
    message: &[u8; 32],
    slot: u32,
) -> Result<SingleMessageAggregate> {
    type_2_setup();

    let raw: Vec<_> = raw_xmss
        .iter()
        .map(|(public_key, signature)| {
            Ok((to_lib_public_key(public_key)?, to_lib_signature(signature)?))
        })
        .collect::<Result<Vec<_>>>()?;

    aggregate_single_message_signatures(children, raw, *message, slot, LOG_INV_RATE)
        .map_err(|err| anyhow!("single-message aggregate aggregation failed: {err}"))
}

pub fn type_1_verify(proof: &SingleMessageAggregate) -> Result<()> {
    type_2_setup_verifier();
    verify_single_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("single-message aggregate verification failed: {err}"))
}

pub fn type_2_merge(parts: Vec<SingleMessageAggregate>) -> Result<MultiMessageAggregate> {
    type_2_setup();
    merge_single_message_aggregates(parts, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate merge failed: {err}"))
}

pub fn type_2_to_wire(proof: &MultiMessageAggregate) -> Vec<u8> {
    proof.to_bytes()
}

pub fn type_2_from_wire(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
) -> Result<MultiMessageAggregate> {
    type_2_setup_verifier();
    let proof = MultiMessageAggregate::from_bytes(wire).ok_or_else(|| {
        anyhow!("Failed to decode multi-message aggregate multi-signature from wire bytes")
    })?;
    if proof.info.len() != public_keys_per_component.len() {
        return Err(anyhow!(
            "Multi-message aggregate has {} components but {} public key sets were expected",
            proof.info.len(),
            public_keys_per_component.len()
        ));
    }
    for (component, public_keys) in proof.info.iter().zip(public_keys_per_component.iter()) {
        ensure_expected_pubkeys(&component.pubkeys, public_keys)?;
    }
    Ok(proof)
}

pub fn type_2_verify(proof: &MultiMessageAggregate) -> Result<()> {
    type_2_setup_verifier();
    verify_multi_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err}"))
}

pub fn type_2_split(proof: MultiMessageAggregate, index: usize) -> Result<SingleMessageAggregate> {
    type_2_setup();
    split_multi_message_aggregate(proof, index, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate split failed: {err}"))
}

pub fn type_2_verify_block(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    let proof = type_2_from_wire(wire, public_keys_per_component)?;

    if proof.info.len() != expected_bindings.len() {
        return Err(anyhow!(
            "Block proof has {} components but {} bindings were expected",
            proof.info.len(),
            expected_bindings.len()
        ));
    }

    for (component, (message, slot)) in proof.info.iter().zip(expected_bindings.iter()) {
        if &component.message != message {
            return Err(anyhow!(
                "Block proof component message does not match block body"
            ));
        }
        if component.slot != *slot {
            return Err(anyhow!(
                "Block proof component slot does not match block body"
            ));
        }
    }

    verify_multi_message_aggregate(&proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err}"))
}

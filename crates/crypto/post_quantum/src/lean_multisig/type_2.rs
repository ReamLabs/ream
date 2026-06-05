use anyhow::{Result, anyhow};
use lean_multisig_type2::{
    MultiMessageAggregate, SingleMessageAggregate, XmssPublicKey, XmssSignature, aggregate_type_1,
    merge_many_type_1, setup_prover, split_type_2, verify_type_1, verify_type_2,
};

use crate::leansig::{public_key::PublicKey, signature::Signature};

pub const LOG_INV_RATE: usize = 2;

pub fn type_2_setup() {
    setup_prover();
}

fn to_lib_public_key(public_key: &PublicKey) -> Result<XmssPublicKey> {
    public_key.as_lean_sig()
}

fn to_lib_signature(signature: &Signature) -> Result<XmssSignature> {
    signature.as_lean_sig()
}

pub fn type_1_from_wire(wire: &[u8], public_keys: &[PublicKey]) -> Result<SingleMessageAggregate> {
    let lib_public_keys = public_keys
        .iter()
        .map(to_lib_public_key)
        .collect::<Result<Vec<_>>>()?;
    SingleMessageAggregate::decompress_without_pubkeys(wire, lib_public_keys).ok_or_else(|| {
        anyhow!("Failed to decode single-message aggregate multi-signature from wire bytes")
    })
}

pub fn type_1_to_wire(proof: &SingleMessageAggregate) -> Vec<u8> {
    proof.compress_without_pubkeys()
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

    aggregate_type_1(children, raw, *message, slot, LOG_INV_RATE)
        .map_err(|err| anyhow!("single-message aggregate aggregation failed: {err:?}"))
}

pub fn type_1_verify(proof: &SingleMessageAggregate) -> Result<()> {
    type_2_setup();
    verify_type_1(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("single-message aggregate verification failed: {err:?}"))
}

pub fn type_2_merge(parts: Vec<SingleMessageAggregate>) -> Result<MultiMessageAggregate> {
    type_2_setup();
    merge_many_type_1(parts, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate merge failed: {err:?}"))
}

pub fn type_2_to_wire(proof: &MultiMessageAggregate) -> Vec<u8> {
    proof.compress_without_pubkeys()
}

pub fn type_2_from_wire(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
) -> Result<MultiMessageAggregate> {
    let lib_public_keys = public_keys_per_component
        .iter()
        .map(|public_keys| {
            public_keys
                .iter()
                .map(to_lib_public_key)
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    MultiMessageAggregate::decompress_without_pubkeys(wire, lib_public_keys).ok_or_else(|| {
        anyhow!("Failed to decode multi-message aggregate multi-signature from wire bytes")
    })
}

pub fn type_2_verify(proof: &MultiMessageAggregate) -> Result<()> {
    type_2_setup();
    verify_type_2(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}

pub fn type_2_split(proof: MultiMessageAggregate, index: usize) -> Result<SingleMessageAggregate> {
    type_2_setup();
    split_type_2(proof, index, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate split failed: {err:?}"))
}

pub fn type_2_verify_block(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    type_2_setup();

    let proof = type_2_from_wire(wire, public_keys_per_component)?;

    if proof.info.len() != expected_bindings.len() {
        return Err(anyhow!(
            "Block proof has {} components but {} bindings were expected",
            proof.info.len(),
            expected_bindings.len()
        ));
    }

    for (component, (message, slot)) in proof.info.iter().zip(expected_bindings.iter()) {
        if &component.without_pubkeys.message != message {
            return Err(anyhow!(
                "Block proof component message does not match block body"
            ));
        }
        if component.without_pubkeys.slot != *slot {
            return Err(anyhow!(
                "Block proof component slot does not match block body"
            ));
        }
    }

    verify_type_2(&proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}

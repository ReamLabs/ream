use anyhow::{Result, anyhow};
use lean_multisig_type2::{
    TypeOneMultiSignature, TypeTwoMultiSignature, XmssPublicKey, XmssSignature, aggregate_type_1,
    merge_many_type_1, setup_prover, split_type_2, verify_type_1, verify_type_2,
};

use crate::leansig::{public_key::PublicKey, signature::Signature};

pub const LOG_INV_RATE: usize = 2;

pub fn type2_setup() {
    setup_prover();
}

fn to_lib_public_key(public_key: &PublicKey) -> Result<XmssPublicKey> {
    public_key.as_lean_sig()
}

fn to_lib_signature(signature: &Signature) -> Result<XmssSignature> {
    signature.as_lean_sig()
}

pub fn type1_from_wire(wire: &[u8], public_keys: &[PublicKey]) -> Result<TypeOneMultiSignature> {
    let lib_public_keys = public_keys
        .iter()
        .map(to_lib_public_key)
        .collect::<Result<Vec<_>>>()?;
    TypeOneMultiSignature::decompress_without_pubkeys(wire, lib_public_keys)
        .ok_or_else(|| anyhow!("Failed to decode Type-1 multi-signature from wire bytes"))
}

pub fn type1_to_wire(proof: &TypeOneMultiSignature) -> Vec<u8> {
    proof.compress_without_pubkeys()
}

pub fn type1_aggregate(
    children: &[TypeOneMultiSignature],
    raw_xmss: &[(PublicKey, Signature)],
    message: &[u8; 32],
    slot: u32,
) -> Result<TypeOneMultiSignature> {
    type2_setup();

    let raw: Vec<_> = raw_xmss
        .iter()
        .map(|(public_key, signature)| {
            Ok((to_lib_public_key(public_key)?, to_lib_signature(signature)?))
        })
        .collect::<Result<Vec<_>>>()?;

    aggregate_type_1(children, raw, *message, slot, LOG_INV_RATE)
        .map_err(|err| anyhow!("Type-1 aggregation failed: {err:?}"))
}

pub fn type1_verify(proof: &TypeOneMultiSignature) -> Result<()> {
    type2_setup();
    verify_type_1(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("Type-1 verification failed: {err:?}"))
}

pub fn type2_merge(parts: Vec<TypeOneMultiSignature>) -> Result<TypeTwoMultiSignature> {
    type2_setup();
    merge_many_type_1(parts, LOG_INV_RATE).map_err(|err| anyhow!("Type-2 merge failed: {err:?}"))
}

pub fn type2_to_wire(proof: &TypeTwoMultiSignature) -> Vec<u8> {
    proof.compress_without_pubkeys()
}

pub fn type2_from_wire(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
) -> Result<TypeTwoMultiSignature> {
    let lib_public_keys = public_keys_per_component
        .iter()
        .map(|public_keys| {
            public_keys
                .iter()
                .map(to_lib_public_key)
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    TypeTwoMultiSignature::decompress_without_pubkeys(wire, lib_public_keys)
        .ok_or_else(|| anyhow!("Failed to decode Type-2 multi-signature from wire bytes"))
}

pub fn type2_verify(proof: &TypeTwoMultiSignature) -> Result<()> {
    type2_setup();
    verify_type_2(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("Type-2 verification failed: {err:?}"))
}

pub fn type2_split(proof: TypeTwoMultiSignature, index: usize) -> Result<TypeOneMultiSignature> {
    type2_setup();
    split_type_2(proof, index, LOG_INV_RATE).map_err(|err| anyhow!("Type-2 split failed: {err:?}"))
}

pub fn type2_verify_block(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    type2_setup();

    let proof = type2_from_wire(wire, public_keys_per_component)?;

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
        .map_err(|err| anyhow!("Type-2 verification failed: {err:?}"))
}

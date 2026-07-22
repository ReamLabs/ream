use anyhow::{Result, anyhow};
use lean_multisig_type2::{
    MultiMessageAggregateSignature as MultiMessageAggregate,
    SingleMessageAggregateSignature as SingleMessageAggregate, XmssPublicKey, XmssSignature,
    aggregate_single_message_signatures, merge_single_message_aggregates, setup_prover,
    setup_verifier, split_multi_message_aggregate, verify_multi_message_aggregate,
    verify_single_message_aggregate,
};

use crate::leansig::{public_key::PublicKey, signature::Signature};
#[cfg(feature = "shadow-integration")]
use crate::shadow::shadow_cost;

pub const LOG_INV_RATE: usize = 2;

pub fn type_2_setup() {
    setup_prover();
}

/// Initialize only the aggregation bytecode needed to *verify* aggregates.
/// Skips the prover-only DFT twiddle precomputation.
pub fn type_2_setup_verifier() {
    setup_verifier();
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
    // Fake-XMSS: hand back a stamped husk clone instead of proving.
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        shadow_cost::sleep(shadow_cost::aggregate_delay(children.len() + raw_xmss.len()));
        return Ok(shadow_prototype::stamped_type_1(message, slot));
    }

    type_2_setup();

    let raw: Vec<_> = raw_xmss
        .iter()
        .map(|(public_key, signature)| {
            Ok((to_lib_public_key(public_key)?, to_lib_signature(signature)?))
        })
        .collect::<Result<Vec<_>>>()?;

    aggregate_single_message_signatures(children, raw, *message, slot, LOG_INV_RATE)
        .map_err(|err| anyhow!("single-message aggregate aggregation failed: {err:?}"))
}

pub fn type_1_verify(proof: &SingleMessageAggregate) -> Result<()> {
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        shadow_cost::sleep(shadow_cost::verify_delay(proof.info.pubkeys.len()));
        return Ok(());
    }

    type_2_setup();
    verify_single_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("single-message aggregate verification failed: {err:?}"))
}

pub fn type_2_merge(parts: Vec<SingleMessageAggregate>) -> Result<MultiMessageAggregate> {
    // Fake-XMSS: a husk clone carrying the real per-component infos (so the merged
    // proof reports the right component count + `(message, slot)`), no proving.
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        shadow_cost::sleep(shadow_cost::merge_delay(parts.len()));
        let mut husk = shadow_prototype::husk_type_2();
        husk.info = parts.iter().map(|part| part.info.clone()).collect();
        return Ok(husk);
    }

    type_2_setup();
    merge_single_message_aggregates(parts, LOG_INV_RATE)
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
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        shadow_cost::sleep(shadow_cost::verify_delay(proof.info.len()));
        return Ok(());
    }

    type_2_setup();
    verify_multi_message_aggregate(proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}

pub fn type_2_split(proof: MultiMessageAggregate, index: usize) -> Result<SingleMessageAggregate> {
    // Fake-XMSS: stamp the split-out Type-1 husk with the target component's
    // `(message, slot)`, no proving.
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        return Ok(match proof.info.get(index) {
            Some(component) => shadow_prototype::stamped_type_1(
                &component.without_pubkeys.message,
                component.without_pubkeys.slot,
            ),
            None => shadow_prototype::husk_type_1(),
        });
    }

    type_2_setup();
    split_multi_message_aggregate(proof, index, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message aggregate split failed: {err:?}"))
}

pub fn type_2_verify_block(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    #[cfg(feature = "shadow-integration")]
    if shadow_cost::fake_xmss() {
        shadow_cost::sleep(shadow_cost::verify_delay(public_keys_per_component.len()));
        return Ok(());
    }

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

    verify_multi_message_aggregate(&proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err:?}"))
}

/// Shadow sim: fake-XMSS husk prototypes. Compiled only under `shadow-integration`.
///
/// Fake mode can't run the prover, and `SingleMessageAggregate` /
/// `MultiMessageAggregate` can't be built from scratch (their inner `Proof<F>`
/// fields are private), so we embed one real Type-1 and one real Type-2 proof —
/// captured offline decode each once, and hand
/// back clones (with `(message, slot)` / component infos stamped in) instead of
/// proving. The wire form (`compress_without_pubkeys`) stays a valid, decodable,
/// per-`(message, slot)`-deterministic proof, so `from_wire` works unchanged.
/// 
/// This is the best solution other than changing the type_2 api
/// to (bytes) -> bytes, rather than using SingleMessageAggregate/Multi-MessageAggregate
/// which can be converted to bytes but has validation checks which arbitrary
/// bytes won't pass
#[cfg(feature = "shadow-integration")]
mod shadow_prototype {
    use std::sync::OnceLock;

    use super::{MultiMessageAggregate, SingleMessageAggregate};

    // Regenerate + replace these if the aggregation circuit changes; a stale blob
    // makes `decompress` below return `None` → a loud startup panic.
    static TYPE_1_BYTES: &[u8] = include_bytes!("../shadow/type1_prototype.bin");
    static TYPE_2_BYTES: &[u8] = include_bytes!("../shadow/type2_prototype.bin");

    static TYPE_1: OnceLock<SingleMessageAggregate> = OnceLock::new();
    static TYPE_2: OnceLock<MultiMessageAggregate> = OnceLock::new();

    fn type_1() -> &'static SingleMessageAggregate {
        TYPE_1.get_or_init(|| {
            // decode calls `get_aggregation_bytecode()`, which requires the
            // aggregation bytecode to be initialized first.
            super::type_2_setup_verifier();
            SingleMessageAggregate::decompress(TYPE_1_BYTES)
                .expect("shadow Type-1 Prototype error")
        })
    }

    fn type_2() -> &'static MultiMessageAggregate {
        TYPE_2.get_or_init(|| {
            super::type_2_setup_verifier();
            MultiMessageAggregate::decompress(TYPE_2_BYTES)
                .expect("shadow Type-2 Prototype error")
        })
    }

    /// Type-1 husk with `(message, slot)` stamped in, so `type_1_to_wire` yields
    /// per-attestation, cross-node-deterministic bytes.
    pub(super) fn stamped_type_1(message: &[u8; 32], slot: u32) -> SingleMessageAggregate {
        let mut husk = type_1().clone();
        husk.info.without_pubkeys.message = *message;
        husk.info.without_pubkeys.slot = slot;
        husk
    }

    pub(super) fn husk_type_1() -> SingleMessageAggregate {
        type_1().clone()
    }

    pub(super) fn husk_type_2() -> MultiMessageAggregate {
        type_2().clone()
    }

    #[cfg(test)]
    mod tests {
        /// Fails loudly if a prototype blob is still a placeholder or is stale for
        /// the current aggregation circuit.
        #[test]
        fn embedded_prototypes_decode() {
            let _ = super::type_1();
            let _ = super::type_2();
        }
    }
}


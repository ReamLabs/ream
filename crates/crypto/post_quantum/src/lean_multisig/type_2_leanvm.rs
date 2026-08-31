use anyhow::{Result, anyhow};
use lean_multisig_optimized::{
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

fn to_lib_public_keys(expected: &[PublicKey]) -> Result<Vec<XmssPublicKey>> {
    expected.iter().map(to_lib_public_key).collect()
}

pub fn type_1_from_wire(wire: &[u8], public_keys: &[PublicKey]) -> Result<SingleMessageAggregate> {
    type_2_setup_verifier();
    SingleMessageAggregate::from_bytes_without_pubkeys(wire, to_lib_public_keys(public_keys)?)
        .ok_or_else(|| {
            anyhow!("Failed to decode single-message aggregate multi-signature from wire bytes")
        })
}

pub fn type_1_to_wire(proof: &SingleMessageAggregate) -> Vec<u8> {
    proof.to_bytes_without_pubkeys()
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
    proof.to_bytes_without_pubkeys()
}

pub fn type_2_from_wire(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
) -> Result<MultiMessageAggregate> {
    type_2_setup_verifier();
    let pubkeys_per_component = public_keys_per_component
        .iter()
        .map(|public_keys| to_lib_public_keys(public_keys))
        .collect::<Result<Vec<_>>>()?;
    let expected_components = pubkeys_per_component.len();
    MultiMessageAggregate::from_bytes_without_pubkeys(wire, pubkeys_per_component).ok_or_else(
        || {
            anyhow!(
                "Failed to decode multi-message aggregate multi-signature from wire bytes \
                 ({expected_components} public key sets supplied)"
            )
        },
    )
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
        if &component.core.message != message {
            return Err(anyhow!(
                "Block proof component message does not match block body"
            ));
        }
        if component.core.slot != *slot {
            return Err(anyhow!(
                "Block proof component slot does not match block body"
            ));
        }
    }

    verify_multi_message_aggregate(&proof)
        .map(|_| ())
        .map_err(|err| anyhow!("multi-message aggregate verification failed: {err}"))
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;
    use crate::leansig::private_key::PrivateKey;

    const SLOT: u32 = 0;
    const MESSAGE: [u8; 32] = [7u8; 32];

    // The leanVM prover uses a process-wide arena that permits only one proving
    // job at a time, so these tests must not run concurrently.
    static PROVER: Mutex<()> = Mutex::new(());

    fn prover_lock() -> MutexGuard<'static, ()> {
        PROVER.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn key_pair(seed_byte: u8) -> (PublicKey, PrivateKey) {
        PrivateKey::generate_key_pair_from_seed([seed_byte; 32], SLOT as usize, 1)
    }

    #[test]
    fn type_1_wire_round_trips_without_pubkeys() {
        let _guard = prover_lock();
        let (public_key, private_key) = key_pair(1);
        let signature = private_key.sign(&MESSAGE, SLOT).expect("signing failed");

        let proof = type_1_aggregate(&[], &[(public_key, signature)], &MESSAGE, SLOT)
            .expect("type-1 aggregation failed");

        let wire = type_1_to_wire(&proof);
        let decoded = type_1_from_wire(&wire, &[public_key]).expect("type-1 decode failed");

        assert_eq!(decoded.info.core.message, MESSAGE);
        assert_eq!(decoded.info.core.slot, SLOT);
        type_1_verify(&decoded).expect("decoded type-1 proof must verify");
    }

    #[test]
    fn type_2_wire_round_trips_without_pubkeys() {
        let _guard = prover_lock();
        let (public_key, private_key) = key_pair(2);
        let signature = private_key.sign(&MESSAGE, SLOT).expect("signing failed");

        let component = type_1_aggregate(&[], &[(public_key, signature)], &MESSAGE, SLOT)
            .expect("type-1 aggregation failed");
        let merged = type_2_merge(vec![component]).expect("type-2 merge failed");

        let wire = type_2_to_wire(&merged);
        let public_keys_per_component = vec![vec![public_key]];
        let decoded =
            type_2_from_wire(&wire, &public_keys_per_component).expect("type-2 decode failed");

        assert_eq!(decoded.info.len(), 1);
        type_2_verify(&decoded).expect("decoded type-2 proof must verify");
    }

    #[test]
    fn type_2_wire_rejects_unexpected_pubkeys() {
        let _guard = prover_lock();
        let (public_key, private_key) = key_pair(3);
        let (other_public_key, _) = key_pair(4);
        let signature = private_key.sign(&MESSAGE, SLOT).expect("signing failed");

        let component = type_1_aggregate(&[], &[(public_key, signature)], &MESSAGE, SLOT)
            .expect("type-1 aggregation failed");
        let merged = type_2_merge(vec![component]).expect("type-2 merge failed");
        let wire = type_2_to_wire(&merged);

        let decoded = type_2_from_wire(&wire, &[vec![other_public_key]]);
        assert!(
            decoded.is_err() || type_2_verify(&decoded.unwrap()).is_err(),
            "a proof decoded against the wrong validator set must not verify"
        );
    }
}

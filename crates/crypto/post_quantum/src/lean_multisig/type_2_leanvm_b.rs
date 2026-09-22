use std::sync::{Mutex, MutexGuard};

use anyhow::{Result, anyhow, ensure};
use leanvm_b::{
    ClaimSelection, EthereumProof, SignatureClaims, XmssClaimGroup, aggregate, setup_prover,
    setup_verifier,
    xmss::{XmssPublicKey, XmssSignature},
};

use crate::leansig::{public_key::PublicKey, signature::Signature};

pub const LOG_INV_RATE: usize = 2;

static PROVER: Mutex<()> = Mutex::new(());

#[derive(Clone)]
pub struct SingleMessageAggregate {
    proof: EthereumProof,
    claim: XmssClaimGroup,
}

#[derive(Clone)]
pub struct MultiMessageAggregate {
    proof: EthereumProof,
    components: Vec<XmssClaimGroup>,
}

fn prover_lock() -> MutexGuard<'static, ()> {
    PROVER.lock().unwrap_or_else(|err| err.into_inner())
}

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

fn claim(public_keys: &[PublicKey], message: [u8; 32], epoch: u32) -> Result<XmssClaimGroup> {
    let mut keys = public_keys
        .iter()
        .map(to_lib_public_key)
        .collect::<Result<Vec<_>>>()?;
    keys.sort_unstable();
    keys.dedup();
    ensure!(!keys.is_empty(), "XMSS claim has no public keys");
    Ok(XmssClaimGroup {
        epoch,
        message,
        keys,
    })
}

fn claims(components: &[XmssClaimGroup]) -> SignatureClaims {
    let mut groups = components.to_vec();
    groups.sort_unstable_by_key(|group| (group.epoch, group.message));
    let mut combined: Vec<XmssClaimGroup> = Vec::with_capacity(groups.len());
    for group in groups {
        if let Some(previous) = combined.last_mut()
            && (previous.epoch, previous.message) == (group.epoch, group.message)
        {
            previous.keys.extend(group.keys);
            previous.keys.sort_unstable();
            previous.keys.dedup();
        } else {
            combined.push(group);
        }
    }
    SignatureClaims {
        xmss: combined,
        sphincs: Vec::new(),
    }
}

pub fn type_1_from_wire(
    wire: &[u8],
    public_keys: &[PublicKey],
    message: &[u8; 32],
    epoch: u32,
) -> Result<SingleMessageAggregate> {
    type_2_setup_verifier();
    let claim = claim(public_keys, *message, epoch)?;
    let proof = EthereumProof::from_bytes_without_pubkeys(
        wire,
        SignatureClaims {
            xmss: vec![claim.clone()],
            sphincs: Vec::new(),
        },
    )
    .map_err(|err| anyhow!("Failed to decode LeanVM-B aggregate proof: {err}"))?;
    Ok(SingleMessageAggregate { proof, claim })
}

pub fn type_1_to_wire(proof: &SingleMessageAggregate) -> Vec<u8> {
    proof.proof.to_bytes_without_pubkeys()
}

pub fn type_1_aggregate(
    children: &[SingleMessageAggregate],
    raw_xmss: &[(PublicKey, Signature)],
    message: &[u8; 32],
    epoch: u32,
) -> Result<SingleMessageAggregate> {
    let _guard = prover_lock();
    type_2_setup();

    ensure!(
        children
            .iter()
            .all(|child| child.claim.message == *message && child.claim.epoch == epoch),
        "child aggregate binding does not match the requested message and epoch"
    );
    let raw = raw_xmss
        .iter()
        .map(|(public_key, signature)| {
            Ok((
                to_lib_public_key(public_key)?,
                epoch,
                *message,
                to_lib_signature(signature)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let child_proofs = children
        .iter()
        .map(|child| child.proof.clone())
        .collect::<Vec<_>>();
    let proof = aggregate(&child_proofs, raw, Vec::new(), &[], None, LOG_INV_RATE)
        .map_err(|err| anyhow!("single-message LeanVM-B aggregation failed: {err}"))?;
    ensure!(
        proof.xmss_signers().len() == 1,
        "single-message LeanVM-B aggregate produced an unexpected claim count"
    );
    let claim = proof.xmss_signers()[0].clone();
    ensure!(
        claim.message == *message && claim.epoch == epoch,
        "single-message LeanVM-B aggregate produced an unexpected binding"
    );
    Ok(SingleMessageAggregate { proof, claim })
}

pub fn type_1_verify(proof: &SingleMessageAggregate) -> Result<()> {
    type_2_setup_verifier();
    proof
        .proof
        .verify()
        .map_err(|err| anyhow!("single-message LeanVM-B verification failed: {err}"))
}

pub fn type_2_merge(parts: Vec<SingleMessageAggregate>) -> Result<MultiMessageAggregate> {
    let _guard = prover_lock();
    type_2_setup();
    let components = parts
        .iter()
        .map(|part| part.claim.clone())
        .collect::<Vec<_>>();
    claims(&components);
    let children = parts.into_iter().map(|part| part.proof).collect::<Vec<_>>();
    let proof = aggregate(&children, Vec::new(), Vec::new(), &[], None, LOG_INV_RATE)
        .map_err(|err| anyhow!("multi-message LeanVM-B merge failed: {err}"))?;
    Ok(MultiMessageAggregate { proof, components })
}

pub fn type_2_to_wire(proof: &MultiMessageAggregate) -> Vec<u8> {
    proof.proof.to_bytes_without_pubkeys()
}

pub fn type_2_from_wire(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<MultiMessageAggregate> {
    type_2_setup_verifier();
    ensure!(
        public_keys_per_component.len() == expected_bindings.len(),
        "public-key-set and binding counts differ"
    );
    let components = public_keys_per_component
        .iter()
        .zip(expected_bindings)
        .map(|(public_keys, (message, epoch))| claim(public_keys, *message, *epoch))
        .collect::<Result<Vec<_>>>()?;
    let proof = EthereumProof::from_bytes_without_pubkeys(wire, claims(&components))
        .map_err(|err| anyhow!("Failed to decode LeanVM-B aggregate proof: {err}"))?;
    Ok(MultiMessageAggregate { proof, components })
}

pub fn type_2_verify(proof: &MultiMessageAggregate) -> Result<()> {
    type_2_setup_verifier();
    ensure!(
        proof.proof.xmss_signers() == claims(&proof.components).xmss,
        "LeanVM-B proof claims do not match expected block components"
    );
    proof
        .proof
        .verify()
        .map_err(|err| anyhow!("multi-message LeanVM-B verification failed: {err}"))
}

pub fn type_2_split(proof: MultiMessageAggregate, index: usize) -> Result<SingleMessageAggregate> {
    let claim = proof
        .components
        .get(index)
        .cloned()
        .ok_or_else(|| anyhow!("LeanVM-B component index {index} is out of bounds"))?;
    let selected = SignatureClaims {
        xmss: vec![claim.clone()],
        sphincs: Vec::new(),
    };
    let _guard = prover_lock();
    type_2_setup();
    let split = aggregate(
        &[proof.proof],
        Vec::new(),
        Vec::new(),
        &[],
        Some(ClaimSelection {
            signatures: &selected,
            da_commitments: &[],
        }),
        LOG_INV_RATE,
    )
    .map_err(|err| anyhow!("LeanVM-B claim-selection split failed: {err}"))?;
    Ok(SingleMessageAggregate {
        proof: split,
        claim,
    })
}

pub fn type_2_verify_block(
    wire: &[u8],
    public_keys_per_component: &[Vec<PublicKey>],
    expected_bindings: &[([u8; 32], u32)],
) -> Result<()> {
    let proof = type_2_from_wire(wire, public_keys_per_component, expected_bindings)?;
    type_2_verify(&proof)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leansig::private_key::PrivateKey;

    fn signed(seed: u8, message: [u8; 32], epoch: u32) -> (PublicKey, Signature) {
        let (public_key, private_key) = PrivateKey::generate_key_pair_from_seed([seed; 32], 0, 2);
        let signature = private_key.sign(&message, epoch).expect("signing failed");
        (public_key, signature)
    }

    #[test]
    fn aggregate_round_trip_verification_and_split_are_bound() {
        let message_a = [7u8; 32];
        let message_b = [8u8; 32];
        let epoch = 0;
        let (public_key_a, signature_a) = signed(1, message_a, epoch);
        let (public_key_a_2, signature_a_2) = signed(2, message_a, epoch);
        let (public_key_b, signature_b) = signed(3, message_b, epoch);
        let (wrong_public_key, _) = signed(4, message_a, epoch);

        let proof_a = type_1_aggregate(
            &[],
            &[(public_key_a, signature_a), (public_key_a_2, signature_a_2)],
            &message_a,
            epoch,
        )
        .expect("first aggregation failed");
        let proof_b = type_1_aggregate(&[], &[(public_key_b, signature_b)], &message_b, epoch)
            .expect("second aggregation failed");
        type_1_verify(&proof_a).expect("first aggregate must verify");

        let wire_a = type_1_to_wire(&proof_a);
        let decoded_a =
            type_1_from_wire(&wire_a, &[public_key_a, public_key_a_2], &message_a, epoch)
                .expect("first aggregate decode failed");
        type_1_verify(&decoded_a).expect("decoded first aggregate must verify");
        for invalid in [
            type_1_from_wire(
                &wire_a,
                &[wrong_public_key, public_key_a_2],
                &message_a,
                epoch,
            ),
            type_1_from_wire(&wire_a, &[public_key_a, public_key_a_2], &message_b, epoch),
            type_1_from_wire(
                &wire_a,
                &[public_key_a, public_key_a_2],
                &message_a,
                epoch + 1,
            ),
        ] {
            assert!(
                invalid.is_err() || type_1_verify(&invalid.unwrap()).is_err(),
                "an aggregate reconstructed with an incorrect binding must not verify"
            );
        }

        let merged = type_2_merge(vec![proof_b, proof_a]).expect("merge failed");
        type_2_verify(&merged).expect("merged aggregate must verify");
        assert_eq!(
            merged
                .proof
                .xmss_signers()
                .iter()
                .map(|group| (group.epoch, group.message, group.keys.len()))
                .collect::<Vec<_>>(),
            vec![(epoch, message_a, 2), (epoch, message_b, 1)]
        );
        let wire = type_2_to_wire(&merged);
        let keys = vec![vec![public_key_b], vec![public_key_a, public_key_a_2]];
        let bindings = vec![(message_b, epoch), (message_a, epoch)];
        let decoded = type_2_from_wire(&wire, &keys, &bindings).expect("merged decode failed");
        type_2_verify(&decoded).expect("decoded merged aggregate must verify");

        for (index, public_keys, message) in [
            (0, vec![public_key_b], message_b),
            (1, vec![public_key_a, public_key_a_2], message_a),
        ] {
            let split = type_2_split(decoded.clone(), index).expect("claim-selection split failed");
            type_1_verify(&split).expect("split aggregate must verify");
            assert_eq!(split.claim.message, message);
            assert_eq!(split.claim.epoch, epoch);
            let split_wire = type_1_to_wire(&split);
            let decoded_split = type_1_from_wire(&split_wire, &public_keys, &message, epoch)
                .expect("split aggregate decode failed");
            type_1_verify(&decoded_split).expect("decoded split aggregate must verify");
        }
        assert!(type_2_split(decoded, 2).is_err());

        for (invalid_keys, invalid_bindings) in [
            (keys.clone(), vec![(message_a, epoch), (message_b, epoch)]),
            (
                keys.clone(),
                vec![(message_b, epoch + 1), (message_a, epoch)],
            ),
            (
                vec![vec![wrong_public_key], vec![public_key_a, public_key_a_2]],
                bindings.clone(),
            ),
        ] {
            let invalid = type_2_from_wire(&wire, &invalid_keys, &invalid_bindings);
            assert!(
                invalid.is_err() || type_2_verify(&invalid.unwrap()).is_err(),
                "an aggregate reconstructed with an incorrect claim must not verify"
            );
        }

        assert!(type_2_from_wire(&wire[..wire.len() / 2], &keys, &bindings).is_err());
    }
}

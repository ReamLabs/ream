use std::path::Path;

use anyhow::{anyhow, bail};
#[cfg(feature = "devnet4")]
use ream_consensus_lean::block::Block;
use ream_consensus_lean::{block::SignedBlock, state::LeanState};
#[cfg(feature = "devnet4")]
use ream_post_quantum_crypto::lean_multisig::type_2::type_2_verify_block;
use tracing::info;
#[cfg(feature = "devnet4")]
use tree_hash::TreeHash;

use crate::types::{TestFixture, verify_signatures::VerifySignaturesTest};

/// Load a verify_signatures test fixture from a JSON file
pub fn load_verify_signatures_test(
    path: impl AsRef<Path>,
) -> anyhow::Result<TestFixture<VerifySignaturesTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .map_err(|err| anyhow!("Failed to read test file {}: {err}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|err| anyhow!("Failed to parse test file {}: {err}", path.display()))
}

/// Run a single verify_signatures test case
pub fn run_verify_signatures_test(
    test_name: &str,
    test: &VerifySignaturesTest,
) -> anyhow::Result<()> {
    info!("Running verify_signatures test: {test_name}");

    let parent_state = LeanState::try_from(&test.anchor_state)
        .map_err(|err| anyhow!("Failed to convert anchor state: {err}"))?;

    #[cfg(feature = "devnet4")]
    if test.signed_block.is_proof_only() {
        return run_proof_signed_block_test(&parent_state, test);
    }

    let signed_block = match SignedBlock::try_from(&test.signed_block) {
        Ok(block) => block,
        Err(err) => {
            // A conversion failure (e.g. malformed signature length) is itself
            // a structural rejection. If the fixture expects an exception,
            // count this as the expected outcome.
            if test.expected_rejection().is_some() {
                info!("Got expected conversion error: {err}");
                return Ok(());
            }
            return Err(anyhow!("Failed to convert signed block: {err}"));
        }
    };

    let result = signed_block.verify_signatures(&parent_state, true);

    match (result, test.expected_rejection()) {
        (Ok(_), Some(exception)) => {
            bail!("Expected exception '{exception}' but verify_signatures succeeded");
        }
        (Err(err), None) => {
            bail!("verify_signatures should succeed but failed: {err}");
        }
        (Err(err), Some(_)) => {
            info!("Got expected exception: {err}");
        }
        (Ok(_), None) => {
            info!("verify_signatures succeeded as expected");
        }
    }
    Ok(())
}

#[cfg(feature = "devnet4")]
fn run_proof_signed_block_test(
    parent_state: &LeanState,
    test: &VerifySignaturesTest,
) -> anyhow::Result<()> {
    let result = verify_proof_signed_block(parent_state, test);

    match (result, test.expected_rejection()) {
        (Ok(_), Some(exception)) => {
            bail!("Expected exception '{exception}' but verify_signatures succeeded");
        }
        (Err(err), None) => {
            bail!("verify_signatures should succeed but failed: {err}");
        }
        (Err(err), Some(_)) => {
            info!("Got expected exception: {err}");
        }
        (Ok(_), None) => {
            info!("verify_signatures succeeded as expected");
        }
    }

    Ok(())
}

#[cfg(feature = "devnet4")]
fn verify_proof_signed_block(
    parent_state: &LeanState,
    test: &VerifySignaturesTest,
) -> anyhow::Result<()> {
    let block = Block::try_from(&test.signed_block.block)
        .map_err(|err| anyhow!("Failed to convert proof signed block: {err}"))?;
    let proof = test
        .signed_block
        .proof
        .as_ref()
        .ok_or_else(|| anyhow!("missing proof signed block proof"))?;
    let proof_bytes = alloy_primitives::hex::decode(proof.data().trim_start_matches("0x"))
        .map_err(|err| anyhow!("Failed to decode proof signed block proof: {err}"))?;

    let validators = &parent_state.validators;
    let mut public_keys_per_component = Vec::with_capacity(block.body.attestations.len() + 1);
    let mut expected_bindings = Vec::with_capacity(block.body.attestations.len() + 1);

    for aggregated_attestation in block.body.attestations.iter() {
        let public_keys = aggregated_attestation
            .aggregation_bits
            .iter()
            .enumerate()
            .filter(|(_, bit)| *bit)
            .map(|(validator_id, _)| {
                validators
                    .get(validator_id)
                    .map(|validator| validator.attestation_public_key)
                    .ok_or_else(|| anyhow!("Validator index out of range"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        public_keys_per_component.push(public_keys);
        expected_bindings.push((
            aggregated_attestation.message.tree_hash_root().into(),
            aggregated_attestation.message.slot as u32,
        ));
    }

    let proposer = validators
        .get(block.proposer_index as usize)
        .ok_or_else(|| anyhow!("Proposer index out of range"))?;
    public_keys_per_component.push(vec![proposer.proposal_public_key]);
    expected_bindings.push((block.tree_hash_root().into(), block.slot as u32));

    type_2_verify_block(&proof_bytes, &public_keys_per_component, &expected_bindings)
}

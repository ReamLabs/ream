use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use alloy_primitives::hex;
use anyhow::{anyhow, bail, ensure};
#[cfg(feature = "devnet4")]
use ream_consensus_lean::attestation::AggregatedSignatureProof;
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::{MultiMessageAggregate, SingleMessageAggregate};
#[cfg(feature = "devnet4")]
use ream_consensus_lean::block::BlockSignatures;
#[cfg(feature = "devnet4")]
use ream_consensus_lean::checkpoint::Checkpoint;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAggregatedAttestation, SignedAttestation},
    block::{Block, SignedBlock},
    state::LeanState,
};
use ream_consensus_misc::constants::lean::INTERVALS_PER_SLOT;
use ream_fork_choice_lean::store::Store;
use ream_network_spec::networks::LeanNetworkSpec;
use ream_post_quantum_crypto::leansig::{private_key::PrivateKey, signature::Signature};
use ream_storage::{
    db::ReamDB,
    dir::setup_data_dir,
    tables::{field::REDBField, table::REDBTable},
};
#[cfg(feature = "devnet5")]
use ssz_types::typenum::U524288;
#[cfg(feature = "devnet4")]
use ssz_types::typenum::U1048576;
use ssz_types::{BitList, VariableList, typenum::U4096};
use tracing::{debug, info};
#[cfg(feature = "devnet4")]
use tree_hash::TreeHash;

use crate::types::{
    TestFixture,
    fork_choice::{AttestationCheck, ForkChoiceStep, ForkChoiceTest, StoreChecks},
};

#[cfg(feature = "devnet4")]
const DEVNET4_MAX_BLOCK_ATTESTATIONS: usize = 16;

/// Load a fork choice test fixture from a JSON file
pub fn load_fork_choice_test(
    path: impl AsRef<Path>,
) -> anyhow::Result<TestFixture<ForkChoiceTest>> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|err| {
        anyhow!(
            "Failed to read test file {:?}: {err}",
            path.as_ref().display()
        )
    })?;

    let fixture: TestFixture<ForkChoiceTest> = serde_json::from_str(&content).map_err(|err| {
        anyhow!(
            "Failed to parse test file {:?}: {err}",
            path.as_ref().display()
        )
    })?;

    Ok(fixture)
}

/// Load test private keys from fixtures/{network}/keys/prod_scheme/{i}.json
fn load_test_keys() -> anyhow::Result<HashMap<u64, PrivateKey>> {
    #[cfg(feature = "devnet5")]
    let keys_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/devnet5/keys/prod_scheme");
    #[cfg(not(feature = "devnet5"))]
    let keys_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/devnet4/keys/prod_scheme");
    let mut keys = HashMap::new();

    for i in 0..12u64 {
        let key_path = keys_dir.join(format!("{i}.json"));
        if !key_path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&key_path)
            .map_err(|err| anyhow!("Failed to read key file {i}.json: {err}"))?;
        let key_json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|err| anyhow!("Failed to parse key file {i}.json: {err}"))?;
        let secret_hex = key_json["secret"]
            .as_str()
            .or_else(|| key_json["proposal_keypair"]["secret_key"].as_str())
            .ok_or_else(|| {
                anyhow!("Missing secret or proposal_keypair.secret_key field in key file {i}.json")
            })?;
        let secret_bytes = hex::decode(secret_hex.trim_start_matches("0x"))
            .map_err(|err| anyhow!("Failed to decode secret hex for validator {i}: {err}"))?;
        let private_key = PrivateKey::from_bytes(&secret_bytes)
            .map_err(|err| anyhow!("Failed to create private key for validator {i}: {err}"))?;
        keys.insert(i, private_key);
    }

    Ok(keys)
}

/// Run a single fork choice test case
pub async fn run_fork_choice_test(test_name: &str, test: ForkChoiceTest) -> anyhow::Result<()> {
    info!("Running fork choice test: {test_name}");

    #[cfg_attr(feature = "devnet5", allow(unused_variables, unused_mut))]
    let mut keys = load_test_keys()?;

    // Extract values needed before consuming anchor_state
    let anchor_state_slot = test.anchor_state.slot;

    // Initialize network spec if not already set
    let mut network_spec = LeanNetworkSpec::ephemery();
    // For spec tests, use genesis_time from the test fixture's state config
    network_spec.genesis_time = test.anchor_state.config.genesis_time;
    ream_network_spec::networks::set_lean_network_spec(std::sync::Arc::new(network_spec.clone()));

    // Convert anchor state and block
    let state = LeanState::try_from(test.anchor_state)
        .map_err(|err| anyhow!("Failed to convert anchor state: {err}"))?;

    let block = Block::try_from(&test.anchor_block)
        .map_err(|err| anyhow!("Failed to convert anchor block: {err}"))?;

    #[cfg(feature = "devnet4")]
    let source_checkpoint = Checkpoint {
        root: block.tree_hash_root(),
        slot: block.slot,
    };

    // Setup test database
    let test_dir = setup_data_dir("spec_tests", None, true)
        .map_err(|err| anyhow!("Failed to setup test directory: {err}"))?;
    let ream_db = ReamDB::new(test_dir).map_err(|err| anyhow!("Failed to create ReamDB: {err}"))?;
    let db = ream_db
        .init_lean_db()
        .map_err(|err| anyhow!("Failed to initialize LeanDB: {err}"))?;

    // Initialize store with anchor state and block
    let store = Store::get_forkchoice_store(
        SignedBlock {
            block,
            #[cfg(feature = "devnet4")]
            signature: BlockSignatures {
                attestation_signatures: VariableList::empty(),
                proposer_signature: Signature::blank(),
            },
            #[cfg(feature = "devnet5")]
            proof: MultiMessageAggregate::default(),
        },
        state,
        db,
        None,
        Some(0),
    );

    // Current fixtures encode invalid-anchor checks as step-less tests. Treat
    // those as initialization assertions so they cannot silently pass.
    let anchor_valid = test.anchor_valid.unwrap_or(!test.steps.is_empty());

    let mut store = match (anchor_valid, store) {
        (true, Ok(store)) => store,
        (true, Err(err)) => return Err(err),
        (false, Ok(_)) => bail!("Anchor was expected to be invalid but store initialized"),
        (false, Err(_)) => return Ok(()),
    };

    info!("  Network: {}", test.network);
    info!("  Anchor state slot: {}", anchor_state_slot);
    info!("  Anchor block slot: {}", test.anchor_block.slot);
    info!("  Number of steps: {}", test.steps.len());

    // Process each step
    for (index, step) in test.steps.iter().enumerate() {
        match step {
            ForkChoiceStep::Tick { time, interval, .. } => {
                let tick_time = match (time, interval) {
                    (Some(tick), _) => *tick,
                    (None, Some(interval)) => {
                        network_spec.genesis_time
                            + (interval * network_spec.seconds_per_slot)
                                .div_ceil(INTERVALS_PER_SLOT)
                    }
                    (None, None) => bail!("Tick step missing both time and interval fields"),
                };
                debug!("  Step {index}: Tick to time {tick_time}");
                store.on_tick(tick_time, false, true).await?;
            }

            ForkChoiceStep::GossipAggregatedAttestation {
                valid,
                checks,
                attestation,
            } => {
                debug!("  Step {index}: GossipAggregatedAttestation");

                let Some(attestation) = attestation else {
                    debug!("    No attestation payload, skipping");
                    continue;
                };

                let mut participants =
                    BitList::<U4096>::with_capacity(attestation.proof.participants.data.len())
                        .map_err(|err| anyhow!("Failed to create participants BitList: {err:?}"))?;
                for (index, &bit) in attestation.proof.participants.data.iter().enumerate() {
                    participants
                        .set(index, bit)
                        .map_err(|err| anyhow!("Failed to set participant bit {index}: {err:?}"))?;
                }

                let proof_bytes = decode_hex_bytes(&attestation.proof.proof_data.data)?;
                #[cfg(feature = "devnet4")]
                let proof_data = VariableList::<u8, U1048576>::new(proof_bytes)
                    .map_err(|err| anyhow!("Failed to build proof_data list: {err:?}"))?;

                #[cfg(feature = "devnet5")]
                let proof = VariableList::<u8, U524288>::new(proof_bytes)
                    .map_err(|err| anyhow!("Failed to build proof_data list: {err:?}"))?;

                #[cfg(feature = "devnet4")]
                let proof = AggregatedSignatureProof::new(participants, proof_data);

                #[cfg(feature = "devnet5")]
                let proof = SingleMessageAggregate::new(participants, proof);

                let signed = SignedAggregatedAttestation {
                    data: attestation.data.clone(),
                    proof,
                };

                let result = store
                    .validate_attestation(&SignedAttestation {
                        validator_id: 0,
                        message: signed.data.clone(),
                        signature: Signature::blank(),
                    })
                    .await;

                match valid {
                    Some(false) => {
                        if result.is_ok() {
                            bail!(
                                "Aggregated attestation at slot {} should be invalid but was accepted",
                                signed.data.slot
                            );
                        }
                    }
                    _ => {
                        result.map_err(|err| {
                            anyhow!(
                                "Aggregated attestation at slot {} should be valid: {err}",
                                signed.data.slot
                            )
                        })?;
                    }
                }

                if let Some(checks) = checks {
                    validate_checks(&store, checks).await?;
                }
            }

            ForkChoiceStep::Block {
                valid,
                block,
                checks,
            } => {
                debug!(
                    "  Step {index}: Block at slot {} (expect valid: {valid})",
                    block.slot
                );

                let ream_block = Block::try_from(block)
                    .map_err(|err| anyhow!("Failed to convert block: {err}"))?;

                // Advance time to the block's slot before processing
                let time = ream_block.slot * network_spec.seconds_per_slot;
                store.on_tick(time, true, true).await?;

                #[cfg(feature = "devnet4")]
                if ream_block.body.attestations.len() > DEVNET4_MAX_BLOCK_ATTESTATIONS {
                    if *valid {
                        bail!(
                            "Block at slot {} exceeds devnet4 attestation limit of {DEVNET4_MAX_BLOCK_ATTESTATIONS}",
                            block.slot,
                        );
                    }
                    if let Some(checks) = checks {
                        validate_checks(&store, checks).await?;
                    }
                    continue;
                }

                // Get the parent state and parent block to extract the correct checkpoints
                #[cfg(feature = "devnet4")]
                let db = store.store.lock().await;
                #[cfg(feature = "devnet4")]
                let parent_block = db
                    .block_provider()
                    .get(ream_block.parent_root)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Parent block not found for parent_root: {}",
                            ream_block.parent_root
                        )
                    })?;
                #[cfg(feature = "devnet4")]
                let parent_slot = parent_block.block.slot;

                #[cfg(feature = "devnet4")]
                drop(db);

                // Build attestation_signatures with `participants` mirroring each
                // body attestation's `aggregation_bits`. The proof_data is left
                // empty because tests run with signature verification disabled,
                // but participants must be populated so fork choice attributes
                // votes to the correct validators.
                #[cfg(feature = "devnet4")]
                let signatures = {
                    let mut proofs = Vec::with_capacity(ream_block.body.attestations.len());
                    for attestation in ream_block.body.attestations.iter() {
                        let mut participants =
                            BitList::<U4096>::with_capacity(attestation.aggregation_bits.len())
                                .map_err(|err| {
                                    anyhow!("Failed to create participants BitList: {err:?}")
                                })?;
                        for (index, bit) in attestation.aggregation_bits.iter().enumerate() {
                            participants.set(index, bit).map_err(|err| {
                                anyhow!("Failed to set participant bit {index}: {err:?}")
                            })?;
                        }
                        proofs.push(AggregatedSignatureProof::new(
                            participants,
                            VariableList::<u8, U1048576>::new(vec![])
                                .expect("Failed to create empty proof_data"),
                        ));
                    }
                    VariableList::<AggregatedSignatureProof, U4096>::try_from(proofs)
                        .map_err(|err| anyhow!("Failed to create signatures VariableList: {err}"))?
                };

                // Build proposer attestation data and sign with real key
                #[cfg(feature = "devnet4")]
                let proposer_attestation_data = AttestationData {
                    slot: ream_block.slot,
                    head: Checkpoint {
                        root: ream_block.tree_hash_root(),
                        slot: ream_block.slot,
                    },
                    target: Checkpoint {
                        root: ream_block.parent_root,
                        slot: parent_slot,
                    },
                    source: source_checkpoint,
                };

                #[cfg(feature = "devnet4")]
                let proposer_index = ream_block.proposer_index;
                #[cfg(feature = "devnet4")]
                let data_root = proposer_attestation_data.tree_hash_root();
                #[cfg(feature = "devnet4")]
                let proposer_signature = {
                    let key = keys.get_mut(&proposer_index).ok_or_else(|| {
                        anyhow!("No signing key found for proposer validator {proposer_index}")
                    })?;
                    while !key.get_prepared_interval().contains(&ream_block.slot) {
                        key.prepare_signature();
                    }
                    key.sign(&data_root.0, ream_block.slot as u32)
                        .map_err(|err| anyhow!("Failed to sign proposer attestation: {err}"))?
                };

                let result = store
                    .on_block(
                        &SignedBlock {
                            block: ream_block,
                            #[cfg(feature = "devnet4")]
                            signature: BlockSignatures {
                                attestation_signatures: signatures,
                                proposer_signature,
                            },
                            #[cfg(feature = "devnet5")]
                            proof: MultiMessageAggregate::default(),
                        },
                        false, // Don't verify signatures in spec tests (we use blank signatures)
                    )
                    .await;

                if *valid {
                    result.map_err(|err| {
                        anyhow!("Block at slot {} should be valid: {err}", block.slot)
                    })?;
                } else if result.is_ok() {
                    bail!(
                        "Block at slot {} should be invalid but was accepted",
                        block.slot
                    );
                }

                // Validate checks if present
                if let Some(checks) = checks {
                    validate_checks(&store, checks).await?;
                }
            }

            ForkChoiceStep::Attestation {
                valid,
                attestation,
                checks,
                is_aggregator,
            } => {
                debug!(
                    "  Step {index}: Attestation from validator {} (expect valid: {valid})",
                    attestation.validator_id
                );

                // Build the SignedAttestation from fixture data, including a real
                // signature when one is provided; otherwise use a blank signature.
                let signature = match attestation.signature.as_deref() {
                    Some(signature_hex) => {
                        let bytes = decode_hex_bytes(signature_hex)?;
                        Signature::from(bytes.as_slice())
                    }
                    None => Signature::blank(),
                };

                let signed_attestation = SignedAttestation {
                    validator_id: attestation.validator_id,
                    message: attestation.data.clone(),
                    signature,
                };

                // Route through `on_gossip_attestation` so the test exercises the
                // full spec path: attestation-data validation, validator-id range
                // check, signature verification, and - for aggregators - recording
                // the vote in the raw per-validator `attestation_signatures` pool
                // that `location: "signatures"` checks read back.
                let result = store
                    .on_gossip_attestation(signed_attestation, is_aggregator.unwrap_or(false))
                    .await;

                if *valid {
                    result.map_err(|err| {
                        anyhow!(
                            "Attestation from validator {} should be valid: {err}",
                            attestation.validator_id
                        )
                    })?;
                } else if result.is_ok() {
                    bail!(
                        "Attestation from validator {} should be invalid but was accepted",
                        attestation.validator_id
                    );
                }

                if let Some(checks) = checks {
                    validate_checks(&store, checks).await?;
                }
            }

            ForkChoiceStep::Checks { checks } => {
                validate_checks(&store, checks).await?;
            }
        }
    }

    info!("Test passed");
    Ok(())
}

/// Validate store checks
async fn validate_checks(store: &Store, checks: &StoreChecks) -> anyhow::Result<()> {
    let db = store.store.lock().await;

    if let Some(expected_head_slot) = checks.head_slot {
        let head_root = db.head_provider().get()?;
        let head_block = db
            .block_provider()
            .get(head_root)?
            .ok_or_else(|| anyhow!("Head block not found"))?;
        let actual_slot = head_block.block.slot;

        ensure!(
            actual_slot == expected_head_slot,
            "Head slot mismatch: expected {expected_head_slot}, got {actual_slot}"
        );

        debug!("Head slot: {actual_slot}");
    }

    if let Some(expected_head_root) = checks.head_root {
        let actual_head_root = db.head_provider().get()?;
        ensure!(
            actual_head_root == expected_head_root,
            "Head root mismatch: expected {expected_head_root}, got {actual_head_root}"
        );
        debug!("Head root: {actual_head_root}");
    }

    if let Some(expected_time) = checks.time {
        let actual_time = db.time_provider().get()?;
        ensure!(
            actual_time == expected_time,
            "Time mismatch: expected {expected_time}, got {actual_time}"
        );
        debug!("Time: {actual_time}");
    }

    if let Some(expected_justified) = &checks.justified_checkpoint {
        let actual_justified = db.latest_justified_provider().get()?;
        ensure!(
            actual_justified.slot == expected_justified.slot
                && actual_justified.root == expected_justified.root,
            "Justified checkpoint mismatch: expected {expected_justified:?}, got {actual_justified:?}"
        );
        debug!("Justified checkpoint: slot {}", actual_justified.slot);
    }

    if let Some(expected_finalized) = &checks.finalized_checkpoint {
        let actual_finalized = db.latest_finalized_provider().get()?;
        ensure!(
            actual_finalized.slot == expected_finalized.slot
                && actual_finalized.root == expected_finalized.root,
            "Finalized checkpoint mismatch: expected {expected_finalized:?}, got {actual_finalized:?}",
        );
        debug!("Finalized checkpoint: slot {}", actual_finalized.slot);
    }

    // Per-validator attestation checks.
    let signature_checks: Vec<&AttestationCheck> = checks
        .attestation_checks
        .iter()
        .filter(|check| check.location == "signatures")
        .collect();

    if !signature_checks.is_empty() {
        // Map each validator to its highest-slot vote in the named pool.
        let signatures_provider = db.attestation_signatures_provider();
        let data_by_root_provider = db.attestation_data_by_root_provider();
        let mut best_by_validator: HashMap<u64, AttestationData> = HashMap::new();
        for key in signatures_provider.get_keys()? {
            let data = data_by_root_provider.get(key.data_root)?.ok_or_else(|| {
                anyhow!(
                    "attestation_signatures key for validator {} references data root {} \
                     missing from attestation_data_by_root",
                    key.validator_id,
                    key.data_root
                )
            })?;
            match best_by_validator.get(&key.validator_id) {
                Some(existing) if existing.slot >= data.slot => continue,
                _ => {
                    let _ = best_by_validator.insert(key.validator_id, data);
                }
            }
        }

        for check in signature_checks {
            let data = best_by_validator.get(&check.validator).ok_or_else(|| {
                anyhow!(
                    "validator {} not found in attestation_signatures pool",
                    check.validator
                )
            })?;

            // Ensure all validators have the expected head slot.
            if let Some(expected) = check.head_slot {
                ensure!(
                    data.head.slot == expected,
                    "Attestation head slot mismatch for validator {}: expected {expected}, got {}",
                    check.validator,
                    data.head.slot
                );
            }

            // Ensure all validators have the expected source slot.
            if let Some(expected) = check.source_slot {
                ensure!(
                    data.source.slot == expected,
                    "Attestation source slot mismatch for validator {}: expected {expected}, got {}",
                    check.validator,
                    data.source.slot
                );
            }

            // Ensure all validators have the expected target slot.
            if let Some(expected) = check.target_slot {
                ensure!(
                    data.target.slot == expected,
                    "Attestation target slot mismatch for validator {}: expected {expected}, got {}",
                    check.validator,
                    data.target.slot
                );
            }
        }
    }

    Ok(())
}

/// Decode a `0x`-prefixed hex string, accepting an optional `0x` prefix.
fn decode_hex_bytes(value: &str) -> anyhow::Result<Vec<u8>> {
    hex::decode(value.trim_start_matches("0x"))
        .map_err(|err| anyhow!("Failed to decode hex bytes: {err}"))
}

use std::path::Path;

use anyhow::{anyhow, bail, ensure};
use ream_consensus_lean::{
    attestation::{Attestation, AttestationData, SignedAttestation},
    block::{Block, BlockWithAttestation, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    state::LeanState,
};
use ream_fork_choice_lean::store::Store;
use ream_network_spec::networks::LeanNetworkSpec;
use ream_post_quantum_crypto::leansig::signature::Signature;
use ream_storage::{
    db::ReamDB,
    dir::setup_data_dir,
    tables::{field::REDBField, table::REDBTable},
};
use ssz_types::VariableList;
use tracing::{debug, info};
use tree_hash::TreeHash;

use crate::types::{
    TestFixture,
    fork_choice::{ForkChoiceStep, ForkChoiceTest, StoreChecks},
};

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

/// Run a single fork choice test case
pub async fn run_fork_choice_test(test_name: &str, test: ForkChoiceTest) -> anyhow::Result<()> {
    info!("Running fork choice test: {test_name}");

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

    // Create anchor checkpoint for use as source in attestations
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
    let mut store = Store::get_forkchoice_store(
        SignedBlockWithAttestation {
            message: BlockWithAttestation {
                proposer_attestation: Attestation {
                    validator_id: block.proposer_index,
                    data: AttestationData {
                        slot: block.slot,
                        head: Checkpoint {
                            root: block.tree_hash_root(),
                            slot: block.slot,
                        },
                        target: state.latest_justified,
                        source: state.latest_finalized,
                    },
                },
                block,
            },
            signature: VariableList::empty(),
        },
        state,
        db,
        None,
    )?;

    info!("  Network: {}", test.network);
    info!("  Anchor state slot: {}", anchor_state_slot);
    info!("  Anchor block slot: {}", test.anchor_block.slot);
    info!("  Number of steps: {}", test.steps.len());

    // Process each step
    for (index, step) in test.steps.iter().enumerate() {
        match step {
            ForkChoiceStep::Tick { time, .. } => {
                debug!("  Step {index}: Tick to time {time}");
                // Update store time
                let db = store.store.lock().await;
                db.time_provider().insert(*time)?;
            }

            ForkChoiceStep::Block {
                valid,
                block,
                checks,
            } => {
                debug!(
                    "  Step {index}: Block at slot {} (expect valid: {valid})",
                    block.block.slot
                );

                let ream_block = Block::try_from(&block.block)
                    .map_err(|err| anyhow!("Failed to convert block: {err}"))?;

                // Advance time to the block's slot before processing
                let time = ream_block.slot * network_spec.seconds_per_slot;
                store.on_tick(time, true).await?;

                // Get the parent state and parent block to extract the correct checkpoints
                let db = store.store.lock().await;

                let parent_block = db
                    .block_provider()
                    .get(ream_block.parent_root)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Parent block not found for parent_root: {}",
                            ream_block.parent_root
                        )
                    })?;

                let parent_slot = parent_block.message.block.slot;

                drop(db);

                // Create blank signatures for block body attestations + 1 for proposer attestation
                let num_signatures = ream_block.body.attestations.len() + 1;
                let signatures = VariableList::try_from(vec![Signature::blank(); num_signatures])
                    .map_err(|err| {
                    anyhow!("Failed to create signatures VariableList: {err}")
                })?;

                let result = store
                    .on_block(
                        &SignedBlockWithAttestation {
                            message: BlockWithAttestation {
                                proposer_attestation: Attestation {
                                    validator_id: ream_block.proposer_index,
                                    data: AttestationData {
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
                                    },
                                },
                                block: ream_block,
                            },
                            signature: signatures,
                        },
                        false, // Don't verify signatures in spec tests (we use blank signatures)
                    )
                    .await;

                if *valid {
                    result.map_err(|err| {
                        anyhow!("Block at slot {} should be valid: {err}", block.block.slot)
                    })?;
                } else if result.is_ok() {
                    bail!(
                        "Block at slot {} should be invalid but was accepted",
                        block.block.slot
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
            } => {
                debug!(
                    "  Step {index}: Attestation from validator {} (expect valid: {valid})",
                    attestation.validator_id
                );

                let signed_attestation = SignedAttestation {
                    message: Attestation::from(attestation),
                    signature: Signature::blank(),
                };

                // Add attestation to new attestations
                let db = store.store.lock().await;
                let result = db
                    .latest_new_attestations_provider()
                    .insert(signed_attestation.message.validator_id, signed_attestation);

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
            .ok_or_else(|| anyhow::anyhow!("Head block not found"))?;
        let actual_slot = head_block.message.block.slot;

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

    Ok(())
}

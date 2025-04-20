#[macro_export]
macro_rules! test_fork_choice {
    ($path:ident) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $path>] {
                use std::fs;

                use alloy_consensus::Blob;
                use alloy_primitives::{hex, map::HashMap, B256, hex::FromHex};
                use ream_consensus::{
                    attestation::Attestation,
                    attester_slashing::AttesterSlashing,
                    checkpoint::Checkpoint,
                    deneb::{
                        beacon_block::{BeaconBlock, SignedBeaconBlock},
                        beacon_state::BeaconState,
                    },
                    execution_engine::{mock_engine::MockExecutionEngine, rpc_types::get_blobs::BlobAndProofV1}, polynomial_commitments::kzg_proof::KZGProof,
                    blob_sidecar::BlobIdentifier,
                };
                use ream_fork_choice::{
                    handlers::{on_attestation, on_attester_slashing, on_block, on_tick},
                    store::{get_forkchoice_store, Store},
                };
                use rstest::rstest;
                use serde::Deserialize;
                use ssz_derive::{Decode, Encode};
                use ssz_types::{
                    typenum::{self, U1099511627776, U4096}, FixedVector, VariableList
                };
                use tree_hash::TreeHash;

                use super::*;
                use $crate::utils;

                #[derive(Debug, Deserialize)]
                pub struct Tick {
                    pub tick: u64,
                    pub valid: Option<bool>,
                }

                #[derive(Debug, Deserialize)]
                pub struct ShouldOverrideForkchoiceUpdate {
                    pub validator_is_connected: bool,
                    pub result: bool,
                }

                #[derive(Debug, Deserialize)]
                pub struct Head {
                    pub slot: u64,
                    pub root: B256,
                }

                #[derive(Debug, Deserialize)]
                pub struct Checks {
                    pub head: Option<Head>,
                    pub time: Option<u64>,
                    pub justified_checkpoint: Option<Checkpoint>,
                    pub finalized_checkpoint: Option<Checkpoint>,
                    pub proposer_boost_root: Option<B256>,
                    pub get_proposer_head: Option<B256>,
                    pub should_override_forkchoice_update: Option<ShouldOverrideForkchoiceUpdate>,
                }

                #[derive(Debug, Deserialize)]
                pub struct Block {
                    pub block: String,
                    pub blobs: Option<String>,
                    pub proofs: Option<Vec<String>>,
                    pub valid: Option<bool>,
                }

                #[derive(Debug, Deserialize)]
                pub struct AttestationStep {
                    pub attestation: String,
                    pub valid: Option<bool>,
                }

                #[derive(Debug, Deserialize)]
                pub struct AttesterSlashingStep {
                    pub attester_slashing: String,
                    pub valid: Option<bool>,
                }

                #[derive(Deserialize)]
                #[serde(untagged)]
                pub enum ForkChoiceStep {
                    Tick(Tick),
                    Checks { checks: Checks },
                    Block(Block),
                    Attestation(AttestationStep),
                    AttesterSlashing(AttesterSlashingStep),
                }

                #[tokio::test]
                async fn test_fork_choice() {
                    let base_path = format!(
                        "mainnet/tests/mainnet/deneb/fork_choice/{}/pyspec_tests",
                        stringify!($path)
                    );

                    let mock_engine = MockExecutionEngine::new();

                    for entry in std::fs::read_dir(base_path).unwrap() {
                        let entry = entry.unwrap();
                        let case_dir = entry.path();

                        if !case_dir.is_dir() {
                            continue;
                        }

                        let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                        println!("Testing case: {}", case_name);

                        let steps: Vec<ForkChoiceStep> = {
                            let steps_path = case_dir.join("steps.yaml");
                            let content =
                                std::fs::read_to_string(&steps_path).expect("Failed to read steps.yaml");
                            serde_yaml::from_str::<Vec<ForkChoiceStep>>(&content)
                                .expect("Failed to parse steps.yaml")
                        };

                        let anchor_state: BeaconState =
                            utils::read_ssz_snappy(&case_dir.join("anchor_state.ssz_snappy"))
                                .expect("Failed to read anchor_state.ssz_snappy");
                        let anchor_block: BeaconBlock =
                            utils::read_ssz_snappy(&case_dir.join("anchor_block.ssz_snappy"))
                                .expect("Failed to read anchor_block.ssz_snappy");

                        let mut store = get_forkchoice_store(anchor_state, anchor_block)
                            .expect("get_forkchoice_store failed");

                        for step in steps {
                            match step {
                                ForkChoiceStep::Tick(ticks) => {
                                    assert_eq!(on_tick(&mut store, ticks.tick).is_ok(), ticks.valid.unwrap_or(true), "Unexpected result on on_tick");
                                }
                                ForkChoiceStep::Block(blocks) => {
                                    let block_path = case_dir.join(format!("{}.ssz_snappy", blocks.block));
                                    if !block_path.exists() {
                                        panic!("Test asset not found: {:?}", block_path);
                                    }
                                    let block: SignedBeaconBlock = utils::read_ssz_snappy(&block_path)
                                        .unwrap_or_else(|_| {
                                            panic!("cannot find test asset (block_{blocks:?}.ssz_snappy)")
                                        });

                                    if let (Some(blobs), Some(proof)) = (blocks.blobs, blocks.proofs) {
                                        let blobs_path = case_dir.join(format!("{}.ssz_snappy", blobs));
                                        let blobs: VariableList<Blob, U4096> = utils::read_ssz_snappy(&blobs_path).expect("Could not read blob file.");
                                        let proof: Vec<KZGProof> = proof
                                            .into_iter()
                                            .map(|proof| KZGProof::from_hex(proof).expect("could not get KZGProof"))
                                            .collect();
                                        let blobs_and_proofs = blobs.into_iter().zip(proof.into_iter()).map(|(blob, proof)| BlobAndProofV1 { blob, proof  } ).collect::<Vec<_>>();
                                        for (index, blob_and_proof) in blobs_and_proofs.into_iter().enumerate() {
                                            store.blobs_and_proofs.insert(BlobIdentifier::new(block.message.tree_hash_root(), index as u64), blob_and_proof);
                                        }
                                    }

                                    assert_eq!(on_block(&mut store, &block, &mock_engine).await.is_ok(), blocks.valid.unwrap_or(true), "Unexpected result on on_block");
                                }
                                ForkChoiceStep::Attestation(attestations) => {
                                    let attestation_path =
                                        case_dir.join(format!("{}.ssz_snappy", attestations.attestation));
                                    if !attestation_path.exists() {
                                        panic!("Test asset not found: {:?}", attestation_path);
                                    }
                                    let attestation: Attestation = utils::read_ssz_snappy(&attestation_path)
                                        .unwrap_or_else(|_| {
                                            panic!("cannot find test asset (block_{attestations:?}.ssz_snappy)")
                                        });
                                    assert_eq!(on_attestation(&mut store, attestation, false).is_ok(), attestations.valid.unwrap_or(true), "Unexpected result on on_attestation");
                                }
                                ForkChoiceStep::AttesterSlashing(slashing_step) => {
                                    let slashing_path = case_dir
                                        .join(format!("{}.ssz_snappy", slashing_step.attester_slashing));
                                    if !slashing_path.exists() {
                                        panic!("Test asset not found: {:?}", slashing_path);
                                    }
                                    let slashing: AttesterSlashing = utils::read_ssz_snappy(&slashing_path)
                                        .unwrap_or_else(|_| {
                                            panic!(
                                                "cannot find test asset (block_{slashing_step:?}.ssz_snappy)"
                                            )
                                        });
                                    assert_eq!(on_attester_slashing(&mut store, slashing).is_ok(), slashing_step.valid.unwrap_or(true), "Unexpected result on on_attester_slashing");
                                }
                                ForkChoiceStep::Checks { checks } => {
                                    if let Some(time) = checks.time {
                                        assert_eq!(
                                            store.time, time,
                                            "checks time mismatch in case {case_name}"
                                        );
                                    }
                                    if let Some(justified_checkpoint) = checks.justified_checkpoint {
                                        assert_eq!(
                                            store.justified_checkpoint, justified_checkpoint,
                                            "checks justified_checkpoint mismatch in case {case_name}"
                                        );
                                    }
                                    if let Some(finalized_checkpoint) = checks.finalized_checkpoint {
                                        assert_eq!(
                                            store.finalized_checkpoint, finalized_checkpoint,
                                            "checks finalized_checkpoint mismatch in case {case_name}"
                                        );
                                    }
                                    if let Some(proposer_boost_root) = checks.proposer_boost_root {
                                        assert_eq!(
                                            store.proposer_boost_root, proposer_boost_root,
                                            "checks proposer_boost_root mismatch in case {case_name}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
}

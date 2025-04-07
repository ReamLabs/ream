#[macro_export]
macro_rules! test_fork_choice {
    () => {
        #[cfg(test)]
        #[allow(non_snake_case)]
        mod tests_fork_choice {
            use super::*;
            use rstest::rstest;
            use alloy_primitives::B256;
            use std::fs;
            use ssz_types::{
                typenum::{U1099511627776},
                VariableList,
            };
            use ssz_derive::{Decode, Encode};

            #[derive(Debug, serde::Deserialize)]
            struct Tick {
                tick: usize,
            }

            #[derive(Debug, serde::Deserialize)]
            pub struct Head {
                pub slot: u64,
                pub root: B256,
            }

            #[derive(Debug, serde::Deserialize)]
            pub struct Checks {
                pub time: usize,
                pub head: Head,
                pub justified_checkpoint: Checkpoint,
                pub finalized_checkpoint: Checkpoint,
                pub proposer_boost_root: B256,
            }

            #[derive(Debug, serde::Deserialize)]
            pub struct Block {
                pub block: String,
                pub valid: bool,
            }

            #[derive(Debug, serde::Deserialize)]
            pub struct AttestationStep {
                pub attestation: String,
            }

            #[derive(Debug, serde::Deserialize)]
            #[serde(untagged)]
            enum ForkChoiceStep {
                Tick(Tick),
                Checks(Checks),
                Block(Block),
                Attestation(AttestationStep),
            }

            #[rstest]
            fn test_fork_choice() {
                let base_path = std::env::current_dir()
                    .unwrap()
                    .join("mainnet/tests/mainnet/deneb/fork_choice/{}/pyspec_tests");

                for entry in std::fs::read_dir(base_path).unwrap() {
                    let entry = entry.unwrap();
                    let case_dir = entry.path();

                    if !case_dir.is_dir() {
                        continue;
                    }

                    let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                    println!("Testing case: {}", case_name);

                    let steps: Vec<ForkChoiceStep> = {
                        let meta_path = case_dir.join("steps.yaml");
                        let content =
                            fs::read_to_string(meta_path).expect("Failed to read steps.yaml");
                        serde_yaml::from_str(&content).expect("Failed to parse steps.yaml")
                    };
                    
                    let anchor_state: BeaconState = utils::read_ssz_snappy(&case_dir.join("anchor_state.ssz_snappy")).expect("Failed to read anchor_state.ssz_snappy");
                    let anchor_block: BeaconBlock = utils::read_ssz_snappy(&case_dir.join("anchor_block.ssz_snappy")).expect("Failed to read anchor_block.ssz_snappy");

                    for block_and_attestation in std::fs::read_dir(&case_dir).unwrap() {
                        let file = block_and_attestation.unwrap();
                        let file_path = file.path();
                        let file_name = file.file_name().into_string().unwrap();

                        if file_name.starts_with("attestation_") && file_name.ends_with(".ssz_snappy") {
                            let attestation: Attestation = utils::read_ssz_snappy(&file_path).expect("Failed to read attestation file");
                        }

                        if file_name.starts_with("block_") && file_name.ends_with(".ssz_snappy") {
                            let block: SignedBeaconBlock = utils::read_ssz_snappy(&file_path).expect("Failed to read block file");
                        }
                    }

                    let mut store = get_forkchoice_store(anchor_state.clone(), anchor_block.clone());


                    match (result, inactivity_penalty_deltas) {
                        (Ok(result), Some(expected)) => {
                            assert_eq!(state, expected, "Post state mismatch in case {case_name}");
                        }
                        (Ok(_), None) => {
                            panic!("Test case {case_name} should have failed but succeeded");
                        }
                        (Err(err), Some(_)) => {
                            panic!("Test case {case_name} should have succeeded but failed, err={err:?}");
                        }
                        (Err(_), None) => {
                            // Test should fail and there should be no post state
                            // This is the expected outcome for invalid operations
                        }
                    }
                }
            }
        }
    };
}

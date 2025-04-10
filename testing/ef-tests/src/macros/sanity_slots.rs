#[macro_export]
macro_rules! test_sanity_slots {
    () => {
        #[cfg(test)]
        #[allow(non_snake_case)]
        mod tests_sanity {
            use std::{
                fs,
                path::{Path, PathBuf},
                sync::Arc,
            };

            use ream_consensus::execution_engine::mock_engine::MockExecutionEngine;
            use serde_yaml;
            use tokio::sync::Mutex;

            use super::*;

            #[tokio::test]
            async fn test_sanity_slots() {
                let base_path = std::env::current_dir()
                    .unwrap()
                    .join("mainnet/tests/mainnet/deneb/sanity/slots/pyspec_tests");

                for entry in std::fs::read_dir(&base_path).unwrap() {
                    let entry = entry.unwrap();
                    let case_dir = entry.path();
                    if !case_dir.is_dir() {
                        continue;
                    }

                    let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                    println!("Testing case: {}", case_name);

                    let slot: u64 = {
                        let slot_path = case_dir.join("slots.yaml");
                        let content =
                            fs::read_to_string(slot_path).expect("Failed to read slots.yaml");

                        serde_yaml::from_str::<u64>(&content)
                            .expect("Failed to parse slot number from slots.yaml")
                    };

                    let mut state: BeaconState =
                        utils::read_ssz_snappy(&case_dir.join("pre.ssz_snappy"))
                            .expect("cannot find test asset (pre.ssz_snappy)");

                    let expected_post =
                        utils::read_ssz_snappy::<BeaconState>(&case_dir.join("post.ssz_snappy"));

                    let result = state.process_slots(state.slot + slot);

                    match (result, expected_post) {
                        (Ok(_), Some(expected)) => {
                            assert_eq!(
                                state, expected,
                                "Post state mismatch in case {}",
                                case_name
                            );
                        }
                        (Ok(_), None) => {
                            panic!("Test case {} should have failed but succeeded", case_name);
                        }
                        (Err(err), Some(_)) => {
                            panic!(
                                "Test case {} should have succeeded but failed, err={:?}",
                                case_name, err
                            );
                        }
                        (Err(_), None) => {
                            // Expected: invalid operations result in an error and no post state.
                        }
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! test_sanity_blocks {
    () => {
        #[cfg(test)]
        #[allow(non_snake_case)]
        mod test_sanity_blocks {
            use std::{fs, path::Path};

            use ream_consensus::execution_engine::mock_engine::MockExecutionEngine;
            use serde_yaml;

            use super::*;

            #[derive(Debug, serde::Deserialize)]
            struct MetaData {
                blocks_count: usize,
                bls_setting: Option<usize>,
            }

            #[tokio::test]
            async fn test_sanity_blocks() {
                let base_path = std::env::current_dir()
                    .unwrap()
                    .join("mainnet/tests/mainnet/deneb/sanity/blocks/pyspec_tests");

                let mock_engine = MockExecutionEngine {
                    execution_valid: true,
                };

                for entry in std::fs::read_dir(&base_path).unwrap() {
                    let entry = entry.unwrap();
                    let case_dir = entry.path();
                    if !case_dir.is_dir() {
                        continue;
                    }

                    let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                    println!("Testing case: {}", case_name);

                    let meta: MetaData = {
                        let meta_path = case_dir.join("meta.yaml");
                        let content =
                            fs::read_to_string(meta_path).expect("Failed to read meta.yaml");
                        serde_yaml::from_str(&content).expect("Failed to parse meta.yaml")
                    };

                    let mut state: BeaconState =
                        utils::read_ssz_snappy(&case_dir.join("pre.ssz_snappy"))
                            .expect("cannot find test asset (pre.ssz_snappy)");

                    let validate_result = true;

                    let mut result: Result<(), String> = Ok(());

                    for i in 0..meta.blocks_count {
                        let block_path = case_dir.join(format!("blocks_{}.ssz_snappy", i));
                        if !block_path.exists() {
                            panic!("Test asset not found: {:?}", block_path);
                        }

                        let signed_block: SignedBeaconBlock = utils::read_ssz_snappy(&block_path)
                            .expect(&format!("cannot find test asset (blocks_{i}.ssz_snappy)"));

                        result = state
                            .state_transition(&signed_block, validate_result, &mock_engine)
                            .await
                            .map_err(|err| err.to_string());
                    }

                    let expected_post =
                        utils::read_ssz_snappy::<BeaconState>(&case_dir.join("post.ssz_snappy"));

                    match (result, expected_post) {
                        (Ok(_), Some(expected)) => {
                            let locked_state = state;
                            assert_eq!(
                                locked_state, expected,
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
                            println!(
                                "Test case {} failed as expected, no post state available.",
                                case_name
                            );
                        }
                    }
                }
            }
        }
    };
}

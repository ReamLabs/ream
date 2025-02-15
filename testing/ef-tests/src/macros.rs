#[macro_export]
macro_rules! test_consensus_type {
    ($struct_name:ident) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $struct_name>] {
                use super::*;
                use rstest::rstest;
                use serde_yaml::Value;
                use snap::raw::Decoder;
                use std::str::FromStr;
                use tree_hash::TreeHash;
                use ssz::Decode;
                use ssz::Encode;

                #[rstest]
                #[case("case_0")]
                #[case("case_1")]
                #[case("case_2")]
                #[case("case_3")]
                #[case("case_4")]
                fn test_type(#[case] case: &str) {
                    let path = format!(
                        "mainnet/tests/mainnet/deneb/ssz_static/{}/ssz_random/{case}/",
                        stringify!($struct_name)
                    );

                    // Read and parse hash root
                    let hash_root = {
                        let hash_root_content = std::fs::read_to_string(format!("{path}roots.yaml"))
                            .expect("cannot find test asset");
                        let value: Value = serde_yaml::from_str(&hash_root_content).unwrap();
                        alloy_primitives::B256::from_str(value.get("root").unwrap().as_str().unwrap())
                            .unwrap()
                    };

                    // Deserialize the struct
                    let content = {
                        let value = std::fs::read_to_string(format!("{path}value.yaml"))
                            .expect("cannot find test asset");
                        serde_yaml::from_str::<$struct_name>(&value).unwrap()
                    };

                    // Read and decompress SSZ snappy file
                    let ssz_snappy = std::fs::read(format!("{path}serialized.ssz_snappy")).expect("cannot find test asset");
                    let mut decoder = Decoder::new();
                    let ssz = decoder.decompress_vec(&ssz_snappy).unwrap();

                    // Perform the assertions
                    assert_eq!(ssz, content.as_ssz_bytes());
                    assert_eq!(content, $struct_name::from_ssz_bytes(&ssz).unwrap());
                    assert_eq!(hash_root, content.tree_hash_root());
                }
            }
        }
    };
}

#[macro_export]
macro_rules! test_operation {
    ($operation_name:ident, $operation_object:ty, $input_name:literal, $processing_fn:path) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $processing_fn>] {
                use super::*;
                use rstest::rstest;

                #[rstest]
                fn test_operation() {
                    let base_path = format!(
                        "mainnet/tests/mainnet/deneb/operations/{}/pyspec_tests",
                        stringify!($operation_name)
                    );

                    for entry in std::fs::read_dir(base_path).unwrap() {
                        let entry = entry.unwrap();
                        let case_dir = entry.path();

                        if !case_dir.is_dir() {
                            continue;
                        }

                        let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                        println!("Testing case: {}", case_name);

                        let metadata_path = case_dir.join("meta.yaml");
                        if metadata_path.exists() {
                            // Read and parse meta.yaml
                            let meta_content = std::fs::read_to_string(&metadata_path)
                                .expect("Failed to read meta.yaml");
                            let meta: serde_yaml::Value = serde_yaml::from_str(&meta_content)
                                .expect("Failed to parse meta.yaml");

                            // Skip test if bls_setting is 1
                            // TODO: When BLS is fully implemented, remove this
                            if let Some(bls_setting) = meta.get("bls_setting") {
                                if bls_setting.as_i64() == Some(1) {
                                    println!("Skipping test case {} because bls_setting is 1", case_name);
                                    continue;
                                }
                            }
                        }

                        let pre_state: BeaconState =
                            utils::read_ssz_snappy(&case_dir.join("pre.ssz_snappy")).expect("cannot find test asset(pre.ssz_snappy)");

                        let input: $operation_object =
                            utils::read_ssz_snappy(&case_dir.join($input_name.to_string() + ".ssz_snappy")).expect("cannot find test asset(<input>.ssz_snappy)");

                        let expected_post = utils::read_ssz_snappy::<BeaconState>(&case_dir.join("post.ssz_snappy"));

                        let mut state = pre_state.clone();
                        let result = state.$processing_fn(&input);

                        match (result, expected_post) {
                            (Ok(_), Some(expected)) => {
                                // Compare balances and print differences
                                for (i, (actual, expected)) in state.balances.iter().zip(expected.balances.iter()).enumerate() {
                                    if actual != expected {
                                        println!("Balance mismatch at index {}: actual={}, expected={}", i, actual, expected);
                                    }
                                }
                                assert_eq!(state, expected, "Post state mismatch in case {}", case_name);
                            }
                            (Ok(_), None) => {
                                panic!("Test case {} should have failed but succeeded", case_name);
                            }
                            (Err(e), Some(_)) => {
                                println!("Error: {:?}", e);
                                panic!("Test case {} should have succeeded but failed", case_name);
                            }
                            (Err(_), None) => {
                                // Test should fail and there should be no post state
                                // This is the expected outcome for invalid operations
                            }
                        }
                    }
                }
            }
        }
    };
}

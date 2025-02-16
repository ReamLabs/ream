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
                            // TODO: When BLS is implemented, remove this
                            if let Some(bls_setting) = meta.get("bls_setting") {
                                if bls_setting.as_i64() == Some(1) {
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
        }
    };
}

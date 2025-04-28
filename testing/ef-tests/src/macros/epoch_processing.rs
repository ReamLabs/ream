#[macro_export]
macro_rules! test_epoch_processing {
    ($operation_name:ident, $processing_fn:path) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $processing_fn>] {
                use super::*;
                use rstest::rstest;

                #[rstest]
                fn test_epoch_processing() {
                    let base_path = format!(
                        "mainnet/tests/mainnet/electra/epoch_processing/{}/pyspec_tests",
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

                        let mut state: BeaconState =
                            utils::read_ssz_snappy(&case_dir.join("pre.ssz_snappy")).expect("cannot find test asset(pre.ssz_snappy)");

                        let expected_post = utils::read_ssz_snappy::<BeaconState>(&case_dir.join("post.ssz_snappy"));

                        let result = state.$processing_fn();

                        match (result, expected_post) {
                            (Ok(_), Ok(expected)) => {
                                assert_eq!(state, expected, "Post state mismatch in case {case_name}");
                            }
                            (Ok(_), Err(_)) => {
                                panic!("Test case {case_name} should have failed but succeeded");
                            }
                            (Err(err), Ok(_)) => {
                                panic!("Test case {case_name} should have succeeded but failed, err={err:?}");
                            }
                            (Err(_), Err(_)) => {
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

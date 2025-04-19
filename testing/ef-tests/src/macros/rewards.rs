#[macro_export]
macro_rules! test_rewards {
    ($operation_name:ident, $processing_fn:path) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $operation_name>] {
                use super::*;
                use rstest::rstest;
                use ssz_types::{
                    typenum::{U1099511627776},
                    VariableList,
                };
                use ssz_derive::{Decode, Encode};


                #[derive(Decode, Debug)]
                pub struct Deltas {
                    pub rewards: VariableList<u64, U1099511627776>,
                    pub penalties: VariableList<u64, U1099511627776>,
                }

                #[rstest]
                fn test_rewards() {
                    let base_path = format!(
                        "mainnet/tests/mainnet/deneb/rewards/{}/pyspec_tests",
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

                        let inactivity_penalty_deltas = utils::read_ssz_snappy::<Deltas>(&case_dir.join("inactivity_penalty_deltas.ssz_snappy"));

                        let result = state.$processing_fn();

                        match (result, inactivity_penalty_deltas) {
                            (Ok(result), Ok(expected)) => {
                                assert_eq!(expected.rewards.to_vec(), result.0, "rewards mismatch in case {case_name}");
                                assert_eq!(expected.penalties.to_vec(), result.1, "penalties mismatch in case {case_name}");
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

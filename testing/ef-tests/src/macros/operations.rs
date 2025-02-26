#[macro_export]
macro_rules! test_operation_impl {
    ($operation_name:ident, $operation_object:ty, $input_name:literal, $compute_result:expr) => {{
        let base_path = format!(
            "mainnet/tests/mainnet/deneb/operations/{}/pyspec_tests",
            stringify!($operation_name)
        );
        for entry in std::fs::read_dir(&base_path).unwrap() {
            let entry = entry.unwrap();
            let case_dir = entry.path();
            if !case_dir.is_dir() {
                continue;
            }
            let case_name = case_dir.file_name().unwrap().to_str().unwrap();
            println!("Testing case: {}", case_name);

            let pre_state: Arc<Mutex<BeaconState>> = Arc::new(Mutex::new(
                utils::read_ssz_snappy(&case_dir.join("pre.ssz_snappy"))
                    .expect("cannot find test asset(pre.ssz_snappy)"),
            ));
            let input: $operation_object =
                utils::read_ssz_snappy(&case_dir.join(format!("{}.ssz_snappy", $input_name)))
                    .expect("cannot find test asset(<input>.ssz_snappy)");
            let expected_post =
                utils::read_ssz_snappy::<BeaconState>(&case_dir.join("post.ssz_snappy"));
            let mut state = pre_state.clone();

            // Call the provided closure to compute the result.
            // The closure is expected to return a Future.
            let result = $compute_result(state.clone(), input, case_dir.to_path_buf()).await;
            match (result, expected_post) {
                (Ok(_), Some(expected)) => {
                    assert_eq!(
                        *state.lock().await,
                        expected,
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
    }};
}

#[macro_export]
macro_rules! test_operation {
    // Variant with a processing function provided.
    ($operation_name:ident, $operation_object:ty, $input_name:literal, $processing_fn:path) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $processing_fn>] {
                use super::*;
                use ream_consensus::execution_engine::mock_engine::MockExecutionEngine;
                use ef_tests::test_operation_impl;
                use std::{path::PathBuf, sync::Arc};
                use tokio::sync::Mutex;

                #[tokio::test]
                async fn test_operation() {
                    test_operation_impl!($operation_name, $operation_object, $input_name, |state: Arc<Mutex<BeaconState>>, input: $operation_object, _case_dir: PathBuf| async move {
                        state.lock().await.$processing_fn(&input)
                    });
                }
            }
        }
    };
    // Variant that uses process_execution_payload with a mock engine.
    ($operation_name:ident, $operation_object:ty, $input_name:literal) => {
        #[cfg(test)]
        mod tests_process_execution_payload {
            use super::*;
            use ream_consensus::execution_engine::mock_engine::MockExecutionEngine;
            use ef_tests::test_operation_impl;
            use std::{path::PathBuf, sync::Arc};
            use tokio::sync::Mutex;

            #[tokio::test]
            async fn test_operation() {
                test_operation_impl!($operation_name, $operation_object, $input_name, |state: Arc<Mutex<BeaconState>>, input: $operation_object, case_dir: PathBuf| async move {
                    let mock_engine = MockExecutionEngine::new(&case_dir.as_path().join("execution.yaml"))
                        .expect("remove result");
                    state.lock().await.process_execution_payload(&input, &mock_engine).await
                });
            }
        }
    };
}

#[macro_export]
macro_rules! test_merkle_proof_impl {
    ($path:literal, $struct_name:ty, $input_name:literal, $inclusion_proof_func:ident, $args:tt) => {
        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<tests_ $path _ $struct_name _ $input_name>] {
                use alloy_primitives::B256;
                use rstest::rstest;
                use serde::Deserialize;
                use tree_hash::TreeHash;
                use ssz::Decode;
                use ssz::Encode;
                use super::*;

                #[derive(Debug, Deserialize)]
                struct MerkleProofTest {
                    leaf: B256,
                    leaf_index: u64,
                    branch: Vec<B256>,
                }

                #[rstest]
                fn test_merkle_proof() {
                    let base_path = format!(
                        "mainnet/tests/mainnet/electra/{}/single_merkle_proof/{}",
                        $path,
                        stringify!($struct_name)
                    );

                    for entry in std::fs::read_dir(base_path).unwrap() {
                        let entry = entry.unwrap();
                        let case_dir = entry.path();

                        if !case_dir.is_dir() || !case_dir.file_name().unwrap().to_str().unwrap().starts_with($input_name) {
                            continue;
                        }

                        let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                        println!("Testing case: {}", case_name);

                        // Read and parse proof.yaml
                        let test_data: MerkleProofTest = {
                            let proof_path = case_dir.join("proof.yaml");
                            let content =
                                std::fs::read_to_string(proof_path).expect("Failed to read proof.yaml");
                            serde_yaml::from_str(&content).expect("Failed to parse proof.yaml")
                        };

                        // Read and parse SSZ file
                        let data: $struct_name =
                            utils::read_ssz_snappy(&case_dir.join("object.ssz_snappy"))
                                .expect(&format!("cannot find test asset (object.ssz_snappy)"));

                        // Verify merkle proof
                        let result = is_valid_normalized_merkle_branch(
                            test_data.leaf,
                            &test_data.branch,
                            test_data.leaf_index,
                            data.tree_hash_root(),
                        );
                        assert!(result, "Merkle proof verification should succeed");

                        // Generate merkle proof
                        let branch = data.$inclusion_proof_func$args.expect("Failed to generate merkle proof");
                        assert_eq!(branch, test_data.branch);
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! test_merkle_proof {
    ($path:literal, $struct_name:ty, $input_name:literal, $inclusion_proof_func:tt) => {
        test_merkle_proof_impl!($path, $struct_name, $input_name, $inclusion_proof_func, ());
    };
    ($path:literal, $struct_name:ty, $input_name:literal, $inclusion_proof_func:tt, $index:literal) => {
        test_merkle_proof_impl!(
            $path,
            $struct_name,
            $input_name,
            $inclusion_proof_func,
            ($index)
        );
    };
}

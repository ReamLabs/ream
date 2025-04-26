#[macro_export]
macro_rules! test_merkle_proof {
    () => {
        #[cfg(test)]
        mod tests_merkle_proof {
            use alloy_primitives::B256;
            use rstest::rstest;
            use serde::Deserialize;
            use tree_hash::TreeHash;

            use super::*;

            #[derive(Debug, Deserialize)]
            struct MerkleProofTest {
                leaf: B256,
                leaf_index: u64,
                branch: Vec<B256>,
            }

            #[rstest]
            fn test_merkle_proof() {
                let base_path =
                    "mainnet/tests/mainnet/deneb/merkle_proof/single_merkle_proof/BeaconBlockBody";

                for entry in std::fs::read_dir(base_path).unwrap() {
                    let entry = entry.unwrap();
                    let case_dir = entry.path();

                    if !case_dir.is_dir() {
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

                    // Read and parse block body SSZ
                    let beacon_block_body: BeaconBlockBody =
                        utils::read_ssz_snappy(&case_dir.join("object.ssz_snappy"))
                            .expect(&format!("cannot find test asset (object.ssz_snappy)"));

                    // Verify merkle proof
                    let result = is_valid_normalized_merkle_branch(
                        test_data.leaf,
                        &test_data.branch,
                        test_data.leaf_index,
                        beacon_block_body.tree_hash_root(),
                    );
                    assert!(result, "Merkle proof verification should succeed");

                    // Generate merkle proof
                    let branch = beacon_block_body
                        .blob_kzg_commitment_inclusion_proof(0)
                        .expect("Failed to generate merkle proof");
                    assert_eq!(
                        beacon_block_body.blob_kzg_commitments[0].tree_hash_root(),
                        test_data.leaf
                    );
                    assert_eq!(branch, test_data.branch);
                }
            }
        }
    };
}

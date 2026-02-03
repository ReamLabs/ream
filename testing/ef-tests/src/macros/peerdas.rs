#[macro_export]
macro_rules! test_peerdas_kzg {
    ($path:ident) => {
        paste::paste! {
            #[cfg(test)]
            mod [<tests_ $path>] {
                use super::*;
                use serde::Deserialize;
                use serde_yaml::{Value, to_value};
                use anyhow::{anyhow, Result, Context};
                use ssz::Decode;
                use ssz_types::VariableList;
                use ream_consensus_beacon::data_column_sidecar::Cell;
                use ream_consensus_misc::polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof};
                use ream_execution_rpc_types::get_blobs::Blob;
                use rust_eth_kzg::{DASContext, UsePrecomp, TrustedSetup};
                use ream_polynomial_commitments::handlers::verify_cell_kzg_proof_batch;
                use ream_consensus_beacon::matrix_entry::{compute_cells_and_kzg_proofs, recover_cells_and_kzg_proofs};

                #[derive(Debug, Deserialize)]
                pub struct KzgInput {
                    pub blob: Option<String>,
                    pub commitments: Option<Vec<String>>,
                    pub proof: Option<String>,
                    pub proofs: Option<Vec<String>>,
                    pub cell_indices: Option<Vec<u64>>,
                    pub cells: Option<Vec<String>>,
                }

                #[derive(Debug, Deserialize)]
                pub struct KzgStep {
                    pub input: KzgInput,
                    pub output: Option<Value>,
                }

                #[test]
                fn [<test_ $path>]() -> Result<()> {
                    let path = stringify!($path);
                    let base_path = format!(
                        "mainnet/tests/mainnet/fulu/kzg/{}/kzg-mainnet",
                        path
                    );
                    let context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);

                    for entry in std::fs::read_dir(&base_path).unwrap() {
                        let case_dir = entry?.path();

                        if !case_dir.is_dir() {
                            continue;
                        }

                        let case_name = case_dir.file_name().unwrap().to_str().unwrap();
                        println!("Running test case: {}", case_name);

                        let steps: Vec<KzgStep> = {
                            let content = std::fs::read_to_string(case_dir.join("data.yaml"))?;
                            let value: Value = serde_yaml::from_str(&content)?;
                            if value.is_sequence() { serde_yaml::from_value(value)? } else { vec![serde_yaml::from_value(value)?] }
                        };

                        for (index, step) in steps.into_iter().enumerate() {
                            let input = step.input;
                            let expected_output = step.output;

                            let result: Result<Value> = (|| {
                                match path {
                                    "verify_cell_kzg_proof_batch" => {
                                        let commitments = VariableList::new(input.commitments.expect("No comms").iter().map(|hex| {
                                            KZGCommitment::from_ssz_bytes(&const_hex::decode(hex.strip_prefix("0x").unwrap_or(hex))?).map_err(|err| anyhow!("Failed to decode {err:?}"))
                                        }).collect::<Result<Vec<_>>>()?).map_err(|err| anyhow!("{err:?}"))?;

                                        let cells = VariableList::new(input.cells.expect("No cells").iter().map(|hex| {
                                            Cell::from_ssz_bytes(&const_hex::decode(hex.strip_prefix("0x").unwrap_or(hex))?).map_err(|err| anyhow!("Failed to decode {err:?}"))
                                        }).collect::<Result<Vec<_>>>()?).map_err(|err| anyhow!("{err:?}"))?;

                                        let proofs = VariableList::new(input.proofs.expect("No proofs").iter().map(|hex| {
                                            KZGProof::from_ssz_bytes(&const_hex::decode(hex.strip_prefix("0x").unwrap_or(hex))?).map_err(|err| anyhow!("Failed to decode {err:?}"))
                                        }).collect::<Result<Vec<_>>>()?).map_err(|err| anyhow!("{err:?}"))?;

                                        verify_cell_kzg_proof_batch(&commitments, &input.cell_indices.expect("No indices"), &cells, &proofs)
                                            .map(|boolean_result| to_value(boolean_result).unwrap())
                                            .map_err(|err| anyhow!("{err:?}"))
                                    },
                                    "compute_cells_and_kzg_proofs" => {
                                        let blob_hex = input.blob.as_ref().expect("No blob");
                                        let blob = Blob::from_ssz_bytes(&const_hex::decode(blob_hex.strip_prefix("0x").unwrap_or(blob_hex))?).map_err(|err| anyhow!("Failed to decode {err:?}"))?;

                                        compute_cells_and_kzg_proofs(&blob, &context).map(|(cells, proofs)| {
                                            to_value((
                                                cells.iter().map(|cell| const_hex::encode_prefixed(cell.as_ref())).collect::<Vec<_>>(),
                                                proofs.iter().map(|proof| const_hex::encode_prefixed(proof.0.as_ref())).collect::<Vec<_>>()
                                            )).unwrap()
                                        }).map_err(|err| anyhow!("Failed to get compute_cells_and_kzg_proofs {err:?}"))
                                    },
                                    "recover_cells_and_kzg_proofs" => {
                                        let cells_vec = input.cells.expect("No cells").iter().map(|hex| {
                                            Cell::from_ssz_bytes(&const_hex::decode(hex.strip_prefix("0x").unwrap_or(hex))?).map_err(|err| anyhow!("{err:?}"))
                                        }).collect::<Result<Vec<_>>>()?;

                                        recover_cells_and_kzg_proofs(input.cell_indices.expect("No indices"), cells_vec, &context).map(|(cells, proofs)| {
                                            to_value((
                                                cells.iter().map(|cell| const_hex::encode_prefixed(cell.as_ref())).collect::<Vec<_>>(),
                                                proofs.iter().map(|proof| const_hex::encode_prefixed(proof.0.as_ref())).collect::<Vec<_>>()
                                            )).unwrap()
                                        }).map_err(|err| anyhow!("Failed to get recover_cells_and_kzg_proofs {err:?}"))
                                    },
                                    _ => Err(anyhow!("Unknown function path: {path}")),
                                }
                            })();

                            match (result, expected_output) {
                                (Ok(actual_value), Some(expected)) => {
                                    assert_eq!(actual_value, expected, "Actual value mismatch in case {case_name}");
                                },
                                (Ok(actual_value), None) => {
                                    panic!("Test case {case_name} should have failed but succeeded");
                                },
                                (Err(_), None) => {},
                                (Err(err), Some(_)) => {
                                    panic!("Test case {case_name} should have succeeded but failed, err={err:?}");
                                },
                            }
                        }
                    }
                    Ok(())
                }
            }
        }
    };
}

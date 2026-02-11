use std::path::Path;

use alloy_primitives::{B256, hex};
use anyhow::{anyhow, bail};
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AttestationData, SignedAttestation,
    },
    block::{
        Block, BlockBody, BlockHeader, BlockSignatures, BlockWithAttestation,
        SignedBlockWithAttestation,
    },
    checkpoint::Checkpoint,
    config::Config,
    state::LeanState,
    validator::Validator,
};
use ssz::Encode;
use tracing::{debug, info, warn};
use tree_hash::TreeHash;

use crate::types::{
    TestFixture,
    ssz_test::{
        AggregatedAttestationJSON, AttestationDataJSON, AttestationJSON, BlockBodyJSON,
        BlockHeaderJSON, BlockJSON, BlockSignaturesJSON, BlockWithAttestationJSON, CheckpointJSON,
        ConfigJSON, SSZTest, SignedAttestationJSON, SignedBlockWithAttestationJSON, StateJSON,
        ValidatorJSON,
    },
};

/// Load an SSZ test fixture from a JSON file
pub fn load_ssz_test(path: impl AsRef<Path>) -> anyhow::Result<TestFixture<SSZTest>> {
    let content = std::fs::read_to_string(path.as_ref()).map_err(|err| {
        anyhow!(
            "Failed to read test file {:?}: {err}",
            path.as_ref().display()
        )
    })?;

    let fixture: TestFixture<SSZTest> = serde_json::from_str(&content).map_err(|err| {
        anyhow!(
            "Failed to parse test file {:?}: {err}",
            path.as_ref().display()
        )
    })?;

    Ok(fixture)
}

/// Run a single SSZ test case
pub fn run_ssz_test(test_name: &str, test: &SSZTest) -> anyhow::Result<()> {
    info!("Running SSZ test: {test_name}");
    debug!("  Network: {}", test.network);
    debug!("  Type: {}", test.type_name);

    // Parse expected values
    let expected_serialized = parse_hex_bytes(&test.serialized)?;
    let expected_root = test.root;

    // Run the test based on type - using intermediate JSON types and converting to ream types
    match test.type_name.as_str() {
        "Checkpoint" => {
            run_test::<CheckpointJSON, Checkpoint>(&test.value, &expected_serialized, expected_root)
        }
        "AttestationData" => run_test::<AttestationDataJSON, AttestationData>(
            &test.value,
            &expected_serialized,
            expected_root,
        ),
        "AggregatedAttestation" => run_test::<AggregatedAttestationJSON, AggregatedAttestation>(
            &test.value,
            &expected_serialized,
            expected_root,
        ),
        "Attestation" => run_test::<AttestationJSON, AggregatedAttestations>(
            &test.value,
            &expected_serialized,
            expected_root,
        ),
        "BlockBody" => {
            run_test::<BlockBodyJSON, BlockBody>(&test.value, &expected_serialized, expected_root)
        }
        "BlockHeader" => run_test::<BlockHeaderJSON, BlockHeader>(
            &test.value,
            &expected_serialized,
            expected_root,
        ),
        "Block" => run_test::<BlockJSON, Block>(&test.value, &expected_serialized, expected_root),
        "Config" => {
            run_test::<ConfigJSON, Config>(&test.value, &expected_serialized, expected_root)
        }
        "Validator" => {
            run_test::<ValidatorJSON, Validator>(&test.value, &expected_serialized, expected_root)
        }
        "State" => {
            run_test::<StateJSON, LeanState>(&test.value, &expected_serialized, expected_root)
        }
        // Types without proper TreeHash implementation - only test SSZ serialization
        "SignedAttestation" => run_test_ssz_only::<SignedAttestationJSON, SignedAttestation>(
            &test.value,
            &expected_serialized,
        ),
        "BlockSignatures" => run_test_ssz_only::<BlockSignaturesJSON, BlockSignatures>(
            &test.value,
            &expected_serialized,
        ),
        "BlockWithAttestation" => {
            run_test_ssz_only::<BlockWithAttestationJSON, BlockWithAttestation>(
                &test.value,
                &expected_serialized,
            )
        }
        "SignedBlockWithAttestation" => run_test_ssz_only::<
            SignedBlockWithAttestationJSON,
            SignedBlockWithAttestation,
        >(&test.value, &expected_serialized),
        _ => {
            warn!("Unknown type: {}, skipping", test.type_name);
            Ok(())
        }
    }
}

/// Run a test by deserializing JSON into intermediate type, converting to ream type,
/// then verifying SSZ serialization and tree hash root.
fn run_test<J, T>(
    value: &serde_json::Value,
    expected_serialized: &[u8],
    expected_root: B256,
) -> anyhow::Result<()>
where
    J: serde::de::DeserializeOwned,
    T: for<'a> TryFrom<&'a J, Error = anyhow::Error> + Encode + TreeHash,
{
    // Deserialize into intermediate JSON type
    let json_value: J = serde_json::from_value(value.clone())
        .map_err(|err| anyhow!("Failed to deserialize JSON value: {err}"))?;

    // Convert to ream type
    let typed_value: T = (&json_value).try_into()?;

    // SSZ serialize
    let serialized = typed_value.as_ssz_bytes();
    if serialized != expected_serialized {
        bail!(
            "SSZ serialization mismatch:\n  expected: 0x{}\n  got:      0x{}",
            hex::encode(expected_serialized),
            hex::encode(&serialized)
        );
    }

    // Compute tree hash root
    let root = typed_value.tree_hash_root();
    if root != expected_root {
        bail!("Tree hash root mismatch:\n  expected: {expected_root}\n  got:      {root}");
    }

    Ok(())
}

/// Run a test for types without TreeHash - only verify SSZ serialization.
fn run_test_ssz_only<J, T>(
    value: &serde_json::Value,
    expected_serialized: &[u8],
) -> anyhow::Result<()>
where
    J: serde::de::DeserializeOwned,
    T: for<'a> TryFrom<&'a J, Error = anyhow::Error> + Encode,
{
    // Deserialize into intermediate JSON type
    let json_value: J = serde_json::from_value(value.clone())
        .map_err(|err| anyhow!("Failed to deserialize JSON value: {err}"))?;

    // Convert to ream type
    let typed_value: T = (&json_value).try_into()?;

    // SSZ serialize
    let serialized = typed_value.as_ssz_bytes();
    if serialized != expected_serialized {
        bail!(
            "SSZ serialization mismatch:\n  expected: 0x{}\n  got:      0x{}",
            hex::encode(expected_serialized),
            hex::encode(&serialized)
        );
    }

    Ok(())
}

/// Parse a hex string (with 0x prefix) into bytes
fn parse_hex_bytes(hex_str: &str) -> anyhow::Result<Vec<u8>> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(hex_str).map_err(|err| anyhow!("Failed to parse hex: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_bytes() {
        let bytes = parse_hex_bytes("0xdeadbeef").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);

        let bytes = parse_hex_bytes("deadbeef").unwrap();
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
    }
}

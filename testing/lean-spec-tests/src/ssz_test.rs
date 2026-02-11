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
        AggregatedAttestationJSON, AttestationJSON, BlockBodyJSON, BlockHeaderJSON, BlockJSON,
        BlockSignaturesJSON, BlockWithAttestationJSON, ConfigJSON, SSZTest, SignedAttestationJSON,
        SignedBlockWithAttestationJSON, StateJSON, ValidatorJSON,
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

    let expected_ssz = parse_hex_bytes(&test.serialized)?;
    let expected_root = test.root;

    match test.type_name.as_str() {
        // Types that deserialize directly (snake_case in JSON)
        "Checkpoint" => run_test_direct::<Checkpoint>(&test.value, &expected_ssz, expected_root),
        "AttestationData" => {
            run_test_direct::<AttestationData>(&test.value, &expected_ssz, expected_root)
        }

        // Types with JSON intermediate conversion
        "AggregatedAttestation" => run_test::<AggregatedAttestationJSON, AggregatedAttestation>(
            &test.value,
            &expected_ssz,
            expected_root,
        ),
        "Attestation" => run_test::<AttestationJSON, AggregatedAttestations>(
            &test.value,
            &expected_ssz,
            expected_root,
        ),
        "BlockBody" => {
            run_test::<BlockBodyJSON, BlockBody>(&test.value, &expected_ssz, expected_root)
        }
        "BlockHeader" => {
            run_test::<BlockHeaderJSON, BlockHeader>(&test.value, &expected_ssz, expected_root)
        }
        "Block" => run_test::<BlockJSON, Block>(&test.value, &expected_ssz, expected_root),
        "Config" => run_test::<ConfigJSON, Config>(&test.value, &expected_ssz, expected_root),
        "Validator" => {
            run_test::<ValidatorJSON, Validator>(&test.value, &expected_ssz, expected_root)
        }
        "State" => run_test::<StateJSON, LeanState>(&test.value, &expected_ssz, expected_root),

        // Types without TreeHash - SSZ only
        "SignedAttestation" => run_test_ssz_only::<SignedAttestationJSON, SignedAttestation>(
            &test.value,
            &expected_ssz,
        ),
        "BlockSignatures" => {
            run_test_ssz_only::<BlockSignaturesJSON, BlockSignatures>(&test.value, &expected_ssz)
        }
        "BlockWithAttestation" => {
            run_test_ssz_only::<BlockWithAttestationJSON, BlockWithAttestation>(
                &test.value,
                &expected_ssz,
            )
        }
        "SignedBlockWithAttestation" => run_test_ssz_only::<
            SignedBlockWithAttestationJSON,
            SignedBlockWithAttestation,
        >(&test.value, &expected_ssz),

        _ => {
            warn!("Unknown type: {}, skipping", test.type_name);
            Ok(())
        }
    }
}

/// Run test with JSON intermediate type conversion
fn run_test<J, T>(
    value: &serde_json::Value,
    expected_ssz: &[u8],
    expected_root: B256,
) -> anyhow::Result<()>
where
    J: serde::de::DeserializeOwned,
    T: for<'a> TryFrom<&'a J, Error = anyhow::Error> + Encode + TreeHash,
{
    let json_value: J = serde_json::from_value(value.clone())
        .map_err(|err| anyhow!("Failed to deserialize JSON: {err}"))?;
    let typed_value: T = (&json_value).try_into()?;
    verify_ssz(&typed_value, expected_ssz)?;
    verify_root(&typed_value, expected_root)
}

/// Run test for types that deserialize directly
fn run_test_direct<T: serde::de::DeserializeOwned + Encode + TreeHash>(
    value: &serde_json::Value,
    expected_ssz: &[u8],
    expected_root: B256,
) -> anyhow::Result<()> {
    let typed_value: T = serde_json::from_value(value.clone())
        .map_err(|err| anyhow!("Failed to deserialize JSON: {err}"))?;
    verify_ssz(&typed_value, expected_ssz)?;
    verify_root(&typed_value, expected_root)
}

/// Run test for types without TreeHash (SSZ only)
fn run_test_ssz_only<J, T>(value: &serde_json::Value, expected_ssz: &[u8]) -> anyhow::Result<()>
where
    J: serde::de::DeserializeOwned,
    T: for<'a> TryFrom<&'a J, Error = anyhow::Error> + Encode,
{
    let json_value: J = serde_json::from_value(value.clone())
        .map_err(|err| anyhow!("Failed to deserialize JSON: {err}"))?;
    let typed_value: T = (&json_value).try_into()?;
    verify_ssz(&typed_value, expected_ssz)
}

fn verify_ssz<T: Encode>(value: &T, expected: &[u8]) -> anyhow::Result<()> {
    let actual = value.as_ssz_bytes();
    if actual != expected {
        bail!(
            "SSZ mismatch:\n  expected: 0x{}\n  got:      0x{}",
            hex::encode(expected),
            hex::encode(&actual)
        );
    }
    Ok(())
}

fn verify_root<T: TreeHash>(value: &T, expected: B256) -> anyhow::Result<()> {
    let actual = value.tree_hash_root();
    if actual != expected {
        bail!("TreeHash mismatch:\n  expected: {expected}\n  got:      {actual}");
    }
    Ok(())
}

fn parse_hex_bytes(hex_str: &str) -> anyhow::Result<Vec<u8>> {
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(hex_str).map_err(|err| anyhow!("Failed to parse hex: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_bytes() {
        assert_eq!(
            parse_hex_bytes("0xdeadbeef").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
        assert_eq!(
            parse_hex_bytes("deadbeef").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }
}

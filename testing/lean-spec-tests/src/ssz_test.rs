use std::path::Path;

use alloy_primitives::hex;
use anyhow::{Context, bail};
use ream_consensus_lean::{
    attestation::{
        AggregatedAttestation, AggregatedAttestations, AggregatedSignatureProof, AttestationData,
        SignedAttestation,
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
use ream_post_quantum_crypto::leansig::{public_key::PublicKey, signature::Signature};
use ssz::Encode;
use tracing::{debug, info, warn};

use crate::types::{
    TestFixture,
    ssz_test::{
        AggregatedAttestationJSON, AggregatedSignatureProofJSON, AttestationDataJSON,
        AttestationJSON, BlockBodyJSON, BlockHeaderJSON, BlockJSON, BlockSignaturesJSON,
        BlockWithAttestationJSON, BlocksByRootRequestJSON, BlocksByRootRequestSSZ, CheckpointJSON,
        ConfigJSON, PublicKeyJSON, SSZTest, SignatureJSON, SignedAttestationJSON,
        SignedBlockWithAttestationJSON, StateJSON, StatusJSON, ValidatorJSON,
    },
};

/// Load an SSZ test fixture from a JSON file
pub fn load_ssz_test(path: impl AsRef<Path>) -> anyhow::Result<TestFixture<SSZTest>> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read test file {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse test file {}", path.display()))
}

/// Run a single SSZ test case. Returns Ok(true) if the test ran, Ok(false) if skipped.
pub fn run_ssz_test(test_name: &str, test: &SSZTest) -> anyhow::Result<bool> {
    info!("Running SSZ test: {test_name}");
    debug!("  Network: {}", test.network);
    debug!("  Type: {}", test.type_name);

    let expected_ssz = parse_hex_bytes(&test.serialized)?;

    match test.type_name.as_str() {
        // Consensus containers
        "Checkpoint" => run_test::<CheckpointJSON, Checkpoint>(&test.value, &expected_ssz),
        "AttestationData" => {
            run_test::<AttestationDataJSON, AttestationData>(&test.value, &expected_ssz)
        }
        "AggregatedAttestation" => {
            run_test::<AggregatedAttestationJSON, AggregatedAttestation>(&test.value, &expected_ssz)
        }
        "Attestation" => {
            run_test::<AttestationJSON, AggregatedAttestations>(&test.value, &expected_ssz)
        }
        "BlockBody" => run_test::<BlockBodyJSON, BlockBody>(&test.value, &expected_ssz),
        "BlockHeader" => run_test::<BlockHeaderJSON, BlockHeader>(&test.value, &expected_ssz),
        "Block" => run_test::<BlockJSON, Block>(&test.value, &expected_ssz),
        "Config" => run_test::<ConfigJSON, Config>(&test.value, &expected_ssz),
        "Validator" => run_test::<ValidatorJSON, Validator>(&test.value, &expected_ssz),
        "State" => run_test::<StateJSON, LeanState>(&test.value, &expected_ssz),
        "BlockWithAttestation" => {
            run_test::<BlockWithAttestationJSON, BlockWithAttestation>(&test.value, &expected_ssz)
        }
        "SignedAttestation" => {
            run_test::<SignedAttestationJSON, SignedAttestation>(&test.value, &expected_ssz)
        }
        "BlockSignatures" => {
            run_test::<BlockSignaturesJSON, BlockSignatures>(&test.value, &expected_ssz)
        }
        "SignedBlockWithAttestation" => run_test::<
            SignedBlockWithAttestationJSON,
            SignedBlockWithAttestation,
        >(&test.value, &expected_ssz),

        // Networking containers
        "Status" => run_test::<StatusJSON, StatusJSON>(&test.value, &expected_ssz),
        "BlocksByRootRequest" => {
            run_test::<BlocksByRootRequestJSON, BlocksByRootRequestSSZ>(&test.value, &expected_ssz)
        }

        // XMSS containers
        "Signature" => run_test::<SignatureJSON, Signature>(&test.value, &expected_ssz),
        "PublicKey" => run_test::<PublicKeyJSON, PublicKey>(&test.value, &expected_ssz),
        "AggregatedSignatureProof" => run_test::<
            AggregatedSignatureProofJSON,
            AggregatedSignatureProof,
        >(&test.value, &expected_ssz),

        _ => {
            warn!("Unknown type: {}, skipping", test.type_name);
            return Ok(false);
        }
    }?;

    Ok(true)
}

/// Run SSZ test. J is the JSON type, T is the SSZ-encodable target type.
fn run_test<J, T>(value: &serde_json::Value, expected_ssz: &[u8]) -> anyhow::Result<()>
where
    J: serde::de::DeserializeOwned,
    T: for<'a> TryFrom<&'a J, Error = anyhow::Error> + Encode,
{
    let json_value: J =
        serde_json::from_value(value.clone()).context("Failed to deserialize JSON")?;
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

fn parse_hex_bytes(hex_str: &str) -> anyhow::Result<Vec<u8>> {
    hex::decode(hex_str).context("Failed to parse hex")
}

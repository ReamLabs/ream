use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use utils::test_case::{define_endpoint, define_test_case};
use ream_rpc::handlers::{
    BlobSidecar, BlockRewards, RootResponse, Committee, Genesis, BlockHeaderData,
    SignedBeaconBlockHeader, PendingConsolidation, PendingDeposit, PendingPartialWithdrawal,
    CheckpointData, Fork, RandaoResponse, ValidatorData, ValidatorIdentity, Attestation,
    SignedBeaconBlock,
};
use alloy_primitives::B256; 
use ream_rpc::handlers::{
    blob_sidecar::get_blob_sidecars,
    block::{
        get_block_attestations, get_block_from_id, get_block_rewards, get_block_root, get_genesis,
    },
    committee::get_committees,
    header::{get_headers, get_headers_from_block},
    state::{
        get_pending_consolidations, get_pending_deposits, get_pending_partial_withdrawals,
        get_state_finality_checkpoint, get_state_fork, get_state_randao, get_state_root,
    },
    validator::{
        get_validator_from_state, get_validators_from_state, post_validator_identities_from_state,
        post_validators_from_state,
    },
};

// Define all endpoints at once
define_endpoint!(GetBlobSidecars, (String, Option<Vec<u64>>), Vec<BlobSidecar>);
define_endpoint!(GetBlockRewards, String, BlockRewards);
define_endpoint!(GetBlockRoot, String, RootResponse);
define_endpoint!(GetCommittees, (String, Option<u64>, Option<u64>, Option<u64>), Vec<Committee>);
define_endpoint!(GetGenesis, (), Genesis);
define_endpoint!(GetHeaders, (Option<u64>, Option<String>), Vec<BlockHeaderData>);
define_endpoint!(GetHeadersFromBlock, String, BlockHeaderData);
define_endpoint!(GetPendingConsolidations, String, Vec<PendingConsolidation>);
define_endpoint!(GetPendingDeposits, String, Vec<PendingDeposit>);
define_endpoint!(GetPendingPartialWithdrawals, String, Vec<PendingPartialWithdrawal>);
define_endpoint!(GetStateFinalityCheckpoint, String, CheckpointData);
define_endpoint!(GetStateFork, String, Fork);
define_endpoint!(GetStateRandao, (String, Option<u64>), RandaoResponse);
define_endpoint!(GetStateRoot, String, RootResponse);
define_endpoint!(GetValidatorFromState, (String, String), ValidatorData);
define_endpoint!(GetValidatorsFromState, (String, Option<Vec<String>>, Option<Vec<String>>), Vec<ValidatorData>);
define_endpoint!(PostValidatorIdentitiesFromState, (String, Vec<u64>), Vec<ValidatorIdentity>);
define_endpoint!(PostValidatorsFromState, (String, Vec<u64>, Option<Vec<String>>), Vec<ValidatorData>);
define_endpoint!(GetBlockAttestations, String, Vec<Attestation>);
define_endpoint!(GetBlockFromId, String, SignedBeaconBlock);

// Helper functions remain the same
fn default_validator() -> Validator {
    Validator::new(
        "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f".to_string(),
        "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        32000000000,
        false,
        0,
        0,
        u64::MAX,
        u64::MAX,
    )
}

fn default_validator_data() -> ValidatorData {
    ValidatorData::new(
        1,
        32000000000,
        "active_ongoing".to_string(),
        default_validator(),
    )
}

fn default_checkpoint() -> Checkpoint {
    Checkpoint::new(0, B256::ZERO)
}

fn default_root() -> B256 {
    B256::from([1u8; 32])
}

fn default_randao() -> B256 {
    B256::from([1u8; 32])
}

pub struct BeaconTestCases;

impl BeaconTestCases {
    // Define test cases using macro
    define_test_case!(
        get_blob_sidecars,
        GetBlobSidecars,
        ("head".to_string(), Some(vec![0])),
        vec![BlobSidecar::default()]
    );

    define_test_case!(
        get_block_rewards,
        GetBlockRewards,
        "head".to_string(),
        BlockRewards::new(0, 15, 8, 4, 2, 1)
    );

    define_test_case!(
        get_block_root,
        GetBlockRoot,
        "head".to_string(),
        RootResponse::new(default_root())
    );

    define_test_case!(
        get_committees,
        GetCommittees,
        ("head".to_string(), Some(1), Some(2), Some(3)),
        vec![Committee::new(1, 2, vec![1300])]
    );

    define_test_case!(
        get_genesis,
        GetGenesis,
        (),
        Genesis::default()
    );

    define_test_case!(
        get_headers,
        GetHeaders,
        (Some(1), Some("0x".repeat(64))),
        vec![BlockHeaderData::new(default_root(), true, SignedBeaconBlockHeader::default())]
    );

    define_test_case!(
        get_headers_from_block,
        GetHeadersFromBlock,
        "head".to_string(),
        BlockHeaderData::new(default_root(), true, SignedBeaconBlockHeader::default())
    );

    // Continue with other test cases...
    define_test_case!(
        get_pending_consolidations,
        GetPendingConsolidations,
        "head".to_string(),
        vec![PendingConsolidation::default()]
    );

    define_test_case!(
        get_pending_deposits,
        GetPendingDeposits,
        "head".to_string(),
        vec![PendingDeposit::default()]
    );

    define_test_case!(
        get_pending_partial_withdrawals,
        GetPendingPartialWithdrawals,
        "head".to_string(),
        vec![PendingPartialWithdrawal::default()]
    );

    define_test_case!(
        get_state_finality_checkpoint,
        GetStateFinalityCheckpoint,
        "head".to_string(),
        CheckpointData::new(default_checkpoint(), default_checkpoint(), default_checkpoint())
    );

    define_test_case!(
        get_state_fork,
        GetStateFork,
        "head".to_string(),
        Fork::default()
    );

    define_test_case!(
        get_state_randao,
        GetStateRandao,
        ("head".to_string(), Some(1)),
        RandaoResponse::new(default_randao())
    );

    define_test_case!(
        get_state_root,
        GetStateRoot,
        "head".to_string(),
        RootResponse::new(default_root())
    );

    define_test_case!(
        get_validator_from_state,
        GetValidatorFromState,
        ("head".to_string(), "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f".to_string()),
        default_validator_data()
    );

    define_test_case!(
        get_validators_from_state,
        GetValidatorsFromState,
        (
            "head".to_string(),
            Some(vec!["0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f".to_string(), "1300".to_string()]),
            Some(vec!["active_ongoing".to_string()]),
        ),
        vec![default_validator_data()]
    );

    define_test_case!(
        post_validator_identities_from_state,
        PostValidatorIdentitiesFromState,
        ("head".to_string(), vec![1300]),
        vec![ValidatorIdentity::new(1300, "0x000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".to_string(), 1)]
    );

    define_test_case!(
        post_validators_from_state,
        PostValidatorsFromState,
        ("head".to_string(), vec![1300], Some(vec!["active_ongoing".to_string()])),
        vec![default_validator_data()]
    );

    define_test_case!(
        get_block_attestations,
        GetBlockAttestations,
        "head".to_string(),
        vec![Attestation::default()]
    );

    define_test_case!(
        get_block_from_id,
        GetBlockFromId,
        "head".to_string(),
        SignedBeaconBlock::default()
    );

    // Macro to generate the HashMap insertion
    pub fn test_cases() -> HashMap<String, Box<dyn std::any::Any + Send + Sync>> {
        macro_rules! insert_test_cases {
            ($map:ident, $($name:literal => $fn:ident),* $(,)?) => {
                $(
                    $map.insert($name.to_string(), Box::new(Self::$fn()) as Box<dyn std::any::Any + Send + Sync>);
                )*
            };
        }

        let mut cases = HashMap::new();
        
        insert_test_cases!(cases,
            "get_blob_sidecars" => get_blob_sidecars,
            "get_block_rewards" => get_block_rewards,
            "get_block_root" => get_block_root,
            "get_committees" => get_committees,
            "get_genesis" => get_genesis,
            "get_headers" => get_headers,
            "get_headers_from_block" => get_headers_from_block,
            "get_pending_consolidations" => get_pending_consolidations,
            "get_pending_deposits" => get_pending_deposits,
            "get_pending_partial_withdrawals" => get_pending_partial_withdrawals,
            "get_state_finality_checkpoint" => get_state_finality_checkpoint,
            "get_state_fork" => get_state_fork,
            "get_state_randao" => get_state_randao,
            "get_state_root" => get_state_root,
            "get_validator_from_state" => get_validator_from_state,
            "get_validators_from_state" => get_validators_from_state,
            "post_validator_identities_from_state" => post_validator_identities_from_state,
            "post_validators_from_state" => post_validators_from_state,
            "get_block_attestations" => get_block_attestations,
            "get_block_from_id" => get_block_from_id,
        );
        
        cases
    }
}
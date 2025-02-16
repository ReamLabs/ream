#![cfg(feature = "ef-tests")]

use ef_tests::{test_consensus_type, test_operation, utils};
use ream_consensus::{
    attestation::Attestation,
    attestation_data::AttestationData,
    attester_slashing::AttesterSlashing,
    beacon_block_header::BeaconBlockHeader,
    bls_to_execution_change::{BLSToExecutionChange, SignedBLSToExecutionChange},
    checkpoint::Checkpoint,
    deneb::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
        beacon_state::BeaconState,
        execution_payload::ExecutionPayload,
        execution_payload_header::ExecutionPayloadHeader,
    },
    deposit::Deposit,
    deposit_data::DepositData,
    eth_1_data::Eth1Data,
    fork::Fork,
    fork_data::ForkData,
    historical_batch::HistoricalBatch,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    proposer_slashing::ProposerSlashing,
    signing_data::SigningData,
    sync_aggregate::SyncAggregate,
    sync_committee::SyncCommittee,
    validator::Validator,
    voluntary_exit::{SignedVoluntaryExit, VoluntaryExit},
    withdrawal::Withdrawal,
};

// Testing consensus types
test_consensus_type!(Attestation);
test_consensus_type!(AttestationData);
test_consensus_type!(AttesterSlashing);
test_consensus_type!(BeaconBlock);
test_consensus_type!(BeaconBlockBody);
test_consensus_type!(BeaconBlockHeader);
test_consensus_type!(BeaconState);
test_consensus_type!(BLSToExecutionChange);
test_consensus_type!(Checkpoint);
test_consensus_type!(Deposit);
test_consensus_type!(DepositData);
test_consensus_type!(ExecutionPayload);
test_consensus_type!(ExecutionPayloadHeader);
test_consensus_type!(Eth1Data);
test_consensus_type!(Fork);
test_consensus_type!(ForkData);
test_consensus_type!(HistoricalBatch);
test_consensus_type!(HistoricalSummary);
test_consensus_type!(IndexedAttestation);
test_consensus_type!(ProposerSlashing);
test_consensus_type!(SignedBeaconBlock);
test_consensus_type!(SignedBLSToExecutionChange);
test_consensus_type!(SignedVoluntaryExit);
test_consensus_type!(SigningData);
test_consensus_type!(SyncAggregate);
test_consensus_type!(SyncCommittee);
test_consensus_type!(Validator);
test_consensus_type!(VoluntaryExit);
test_consensus_type!(Withdrawal);

// Testing operations for block processing
test_operation!(deposit, Deposit, "deposit", process_deposit);
test_operation!(
    withdrawals,
    ExecutionPayload,
    "execution_payload",
    process_withdrawals
);

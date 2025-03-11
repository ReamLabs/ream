#![cfg(feature = "ef-tests")]

use ef_tests::{test_consensus_type, test_epoch_processing, test_operation, test_shuffling, utils};
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
    misc::compute_shuffled_index,
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
test_operation!(attestation, Attestation, "attestation", process_attestation);
test_operation!(
    attester_slashing,
    AttesterSlashing,
    "attester_slashing",
    process_attester_slashing
);
test_operation!(block_header, BeaconBlock, "block", process_block_header);
test_operation!(
    bls_to_execution_change,
    SignedBLSToExecutionChange,
    "address_change",
    process_bls_to_execution_change
);
test_operation!(deposit, Deposit, "deposit", process_deposit);
test_operation!(execution_payload, BeaconBlockBody, "body");
test_operation!(
    proposer_slashing,
    ProposerSlashing,
    "proposer_slashing",
    process_proposer_slashing
);
test_operation!(
    voluntary_exit,
    SignedVoluntaryExit,
    "voluntary_exit",
    process_voluntary_exit
);
test_operation!(
    withdrawals,
    ExecutionPayload,
    "execution_payload",
    process_withdrawals
);

// Testing shuffling
test_shuffling!();

// Testing epoch_processing
test_epoch_processing!(effective_balance_updates, process_effective_balance_updates);
test_epoch_processing!(eth1_data_reset, process_eth1_data_reset);
test_epoch_processing!(
    historical_summaries_update,
    process_historical_summaries_update
);
test_epoch_processing!(inactivity_updates, process_inactivity_updates);
test_epoch_processing!(
    participation_flag_updates,
    process_participation_flag_updates
);
test_epoch_processing!(randao_mixes_reset, process_randao_mixes_reset);
test_epoch_processing!(slashings_reset, process_slashings_reset);
test_epoch_processing!(slashings, process_slashings);

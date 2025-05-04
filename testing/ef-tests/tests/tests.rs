#![cfg(feature = "ef-tests")]

use ef_tests::{
    test_consensus_type, test_epoch_processing, test_fork_choice, test_merkle_proof,
    test_merkle_proof_impl, test_operation, test_rewards, test_sanity_blocks, test_sanity_slots,
    test_shuffling, utils,
};
use ream_consensus::{
    attestation::Attestation,
    attestation_data::AttestationData,
    attester_slashing::AttesterSlashing,
    beacon_block_header::BeaconBlockHeader,
    bls_to_execution_change::{BLSToExecutionChange, SignedBLSToExecutionChange},
    checkpoint::Checkpoint,
    consolidation_request::ConsolidationRequest,
    deposit::Deposit,
    deposit_data::DepositData,
    deposit_request::DepositRequest,
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_block_body::BeaconBlockBody,
        beacon_state::BeaconState,
        execution_payload::ExecutionPayload,
        execution_payload_header::ExecutionPayloadHeader,
    },
    eth_1_data::Eth1Data,
    execution_requests::ExecutionRequests,
    fork::Fork,
    fork_data::ForkData,
    historical_batch::HistoricalBatch,
    historical_summary::HistoricalSummary,
    indexed_attestation::IndexedAttestation,
    misc::compute_shuffled_index,
    pending_consolidation::PendingConsolidation,
    pending_deposit::PendingDeposit,
    pending_partial_withdrawal::PendingPartialWithdrawal,
    proposer_slashing::ProposerSlashing,
    signing_data::SigningData,
    single_attestation::SingleAttestation,
    sync_aggregate::SyncAggregate,
    sync_committee::SyncCommittee,
    validator::Validator,
    voluntary_exit::{SignedVoluntaryExit, VoluntaryExit},
    withdrawal::Withdrawal,
    withdrawal_request::WithdrawalRequest,
};
use ream_merkle::is_valid_normalized_merkle_branch;

// General consensus types
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

// Electra consensus types
test_consensus_type!(ConsolidationRequest);
test_consensus_type!(DepositRequest);
test_consensus_type!(ExecutionRequests);
test_consensus_type!(PendingConsolidation);
test_consensus_type!(PendingDeposit);
test_consensus_type!(PendingPartialWithdrawal);
test_consensus_type!(SingleAttestation);
test_consensus_type!(WithdrawalRequest);

// Testing operations
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
test_operation!(
    consolidation_request,
    ConsolidationRequest,
    "consolidation_request",
    process_consolidation_request
);
test_operation!(deposit, Deposit, "deposit", process_deposit);
test_operation!(
    deposit_request,
    DepositRequest,
    "deposit_request",
    process_deposit_request
);
test_operation!(execution_payload, BeaconBlockBody, "body");
test_operation!(
    proposer_slashing,
    ProposerSlashing,
    "proposer_slashing",
    process_proposer_slashing
);
test_operation!(
    sync_aggregate,
    SyncAggregate,
    "sync_aggregate",
    process_sync_aggregate
);
test_operation!(
    voluntary_exit,
    SignedVoluntaryExit,
    "voluntary_exit",
    process_voluntary_exit
);
test_operation!(
    withdrawal_request,
    WithdrawalRequest,
    "withdrawal_request",
    process_withdrawal_request
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
    justification_and_finalization,
    process_justification_and_finalization
);
test_epoch_processing!(
    participation_flag_updates,
    process_participation_flag_updates
);
test_epoch_processing!(pending_consolidations, process_pending_consolidations);
test_epoch_processing!(pending_deposits, process_pending_deposits);
test_epoch_processing!(randao_mixes_reset, process_randao_mixes_reset);
test_epoch_processing!(registry_updates, process_registry_updates);
test_epoch_processing!(rewards_and_penalties, process_rewards_and_penalties);
test_epoch_processing!(slashings, process_slashings);
test_epoch_processing!(slashings_reset, process_slashings_reset);

// Testing rewards
test_rewards!(basic, get_inactivity_penalty_deltas);
test_rewards!(leak, get_inactivity_penalty_deltas);
test_rewards!(random, get_inactivity_penalty_deltas);

// Testing sanity
test_sanity_blocks!(test_sanity_blocks, "sanity/blocks");
test_sanity_slots!();

// Testing fork_choice
test_fork_choice!(ex_ante);
test_fork_choice!(get_head);
test_fork_choice!(get_proposer_head);
test_fork_choice!(on_block);
test_fork_choice!(should_override_forkchoice_update);

// Testing merkle_proof
test_merkle_proof!(
    "light_client",
    BeaconState,
    "current_sync_committee",
    current_sync_committee_inclusion_proof
);
test_merkle_proof!(
    "light_client",
    BeaconState,
    "next_sync_committee",
    next_sync_committee_inclusion_proof
);
test_merkle_proof!(
    "light_client",
    BeaconState,
    "finality_root",
    finalized_root_inclusion_proof
);
test_merkle_proof!(
    "light_client",
    BeaconBlockBody,
    "execution",
    execution_payload_inclusion_proof
);
test_merkle_proof!(
    "merkle_proof",
    BeaconBlockBody,
    "blob_kzg_commitment",
    blob_kzg_commitment_inclusion_proof,
    0
);

// Testing random
test_sanity_blocks!(test_random, "random/random");

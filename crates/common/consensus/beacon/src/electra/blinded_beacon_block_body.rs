use alloy_primitives::B256;
use ream_bls::BLSSignature;
use ream_consensus_misc::eth_1_data::Eth1Data;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{U1, U8, U16, U4096},
};
use tree_hash_derive::TreeHash;

use crate::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    bls_to_execution_change::SignedBLSToExecutionChange, deposit::Deposit,
    electra::execution_payload_header::ExecutionPayloadHeader,
    execution_requests::ExecutionRequests, polynomial_commitments::kzg_commitment::KZGCommitment,
    proposer_slashing::ProposerSlashing, sync_aggregate::SyncAggregate,
    voluntary_exit::SignedVoluntaryExit,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlindedBeaconBlockBody {
    pub randao_reveal: BLSSignature,

    /// Eth1 data vote
    pub eth1_data: Eth1Data,

    /// Arbitrary data
    pub graffiti: B256,

    // Operations
    pub proposer_slashings: VariableList<ProposerSlashing, U16>,
    pub attester_slashings: VariableList<AttesterSlashing, U1>,
    pub attestations: VariableList<Attestation, U8>,
    pub deposits: VariableList<Deposit, U16>,
    pub voluntary_exits: VariableList<SignedVoluntaryExit, U16>,
    pub sync_aggregate: SyncAggregate,

    // Execution
    pub execution_payload_header: ExecutionPayloadHeader,
    pub bls_to_execution_changes: VariableList<SignedBLSToExecutionChange, U16>,
    pub blob_kzg_commitments: VariableList<KZGCommitment, U4096>,
    pub execution_requests: ExecutionRequests,
}

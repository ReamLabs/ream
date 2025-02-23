use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    typenum::{U128, U16, U2, U4096},
    VariableList,
};
use tree_hash_derive::TreeHash;

use super::execution_payload::ExecutionPayload;
use crate::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    bls_to_execution_change::SignedBLSToExecutionChange, deposit::Deposit, eth_1_data::Eth1Data,
    kzg_commitment::KZGCommitment, proposer_slashing::ProposerSlashing,
    sync_aggregate::SyncAggregate, voluntary_exit::SignedVoluntaryExit,
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlockBody {
    pub randao_reveal: BLSSignature,

    /// Eth1 data vote
    pub eth1_data: Eth1Data,

    /// Arbitrary data
    pub graffiti: B256,

    // Operations
    pub proposer_slashings: VariableList<ProposerSlashing, U16>,
    pub attester_slashings: VariableList<AttesterSlashing, U2>,
    pub attestations: VariableList<Attestation, U128>,
    pub deposits: VariableList<Deposit, U16>,
    pub voluntary_exits: VariableList<SignedVoluntaryExit, U16>,
    pub sync_aggregate: SyncAggregate,
    pub execution_payload: ExecutionPayload,
    pub bls_to_execution_changes: VariableList<SignedBLSToExecutionChange, U16>,
    pub blob_kzg_commitments: VariableList<KZGCommitment, U4096>,
}

use std::sync::Arc;

use alloy_primitives::B256;
use libp2p_identity::PeerId;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAggregatedAttestation, SignedAttestation},
    block::{BlockWithSignatures, SignedBlock},
    checkpoint::Checkpoint,
};
use ream_req_resp::lean::NetworkEvent;
use tokio::sync::oneshot;

/// Represents the status of a peer's chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub head: Checkpoint,
    pub finalized: Checkpoint,
}

#[derive(Debug)]
pub enum LeanChainServiceMessage {
    // Producers
    ProduceBlock {
        slot: u64,
        sender: oneshot::Sender<ServiceResponse<BlockWithSignatures>>,
    },
    BuildAttestationData {
        slot: u64,
        sender: oneshot::Sender<ServiceResponse<AttestationData>>,
    },

    // Processors
    ProcessBlock {
        signed_block: Box<SignedBlock>,
        need_gossip: bool,
    },
    ProcessAttestation {
        signed_attestation: Box<SignedAttestation>,
        subnet_id: u64,
        need_gossip: bool,
    },
    ProcessAggregatedAttestation {
        aggregated_attestation: Box<SignedAggregatedAttestation>,
        need_gossip: bool,
    },
    CheckIfCanonicalCheckpoint {
        peer_id: PeerId,
        checkpoint: Checkpoint,
        sender: oneshot::Sender<(PeerId, bool)>,
    },
    GetBlocksByRange {
        start_slot: u64,
        count: u64,
        sender: tokio::sync::mpsc::Sender<Arc<SignedBlock>>,
    },
    GetBlocksByRoot {
        roots: Vec<B256>,
        sender: oneshot::Sender<Vec<Arc<SignedBlock>>>,
    },
    NetworkEvent(NetworkEvent),
}

#[derive(Debug)]
pub struct RequestedBlocksByRoot {
    pub peer_id: PeerId,
    pub roots: Vec<B256>,
}

#[derive(Debug)]
pub struct RequestedStatus {
    pub status: Status,
}

#[derive(Debug)]
pub enum RequestResult<T> {
    Success(T),
    NotConnected,
}

#[derive(Debug)]
pub enum ServiceResponse<T> {
    Ok(T),
    Syncing,
    /// The local view is too far behind wall-clock to safely sign this duty.
    /// Carries the snapshot that drove the decision for log/metric attribution.
    SyncLagGated {
        head_slot: u64,
        lag: u64,
        max_seen_slot: u64,
    },
    Err(anyhow::Error),
}

/// Tag identifying which validator duty is asking the sync-lag gate for a
/// decision. Used only for structured logging and counter attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DutyKind {
    Block,
    Attestation,
}

impl DutyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DutyKind::Block => "block",
            DutyKind::Attestation => "attestation",
        }
    }
}

/// Outcome of consulting the sync-lag duty gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DutyGateDecision {
    /// Duties may run for this slot.
    Allowed,
    /// Duties must be silenced. The snapshot is carried so the call site can
    /// fill the matching `ServiceResponse::SyncLagGated` and increment the
    /// duty-specific counter.
    Gated {
        head_slot: u64,
        lag: u64,
        max_seen_slot: u64,
    },
}

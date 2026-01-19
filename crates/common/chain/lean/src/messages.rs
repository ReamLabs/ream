use std::sync::Arc;

use alloy_primitives::B256;
use libp2p_identity::PeerId;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
};
use ream_req_resp::lean::ReamNetworkEvent;
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
        signed_block_with_attestation: Box<SignedBlockWithAttestation>,
        need_gossip: bool,
    },
    ProcessAttestation {
        signed_attestation: Box<SignedAttestation>,
        need_gossip: bool,
    },
    CheckIfCanonicalCheckpoint {
        peer_id: PeerId,
        checkpoint: Checkpoint,
        sender: oneshot::Sender<(PeerId, bool)>,
    },
    GetBlocksByRoot {
        roots: Vec<B256>,
        sender: oneshot::Sender<Vec<Arc<SignedBlockWithAttestation>>>,
    },
    NetworkEvent(ReamNetworkEvent),
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
}

use ream_bls::BLSSignature;
use ream_consensus_beacon::{
    bls_to_execution_change::BLSToExecutionChange, voluntary_exit::VoluntaryExit,
};
use serde::{Deserialize, Serialize};

/// Voluntary exit event.
///
/// The node has received a SignedVoluntaryExit (from P2P or API) that passes
/// validation rules of the `voluntary_exit` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoluntaryExitEvent {
    pub message: VoluntaryExit,
    pub signature: BLSSignature,
}

/// BLS to execution change event.
///
/// The node has received a SignedBLSToExecutionChange (from P2P or API) that passes
/// validation rules of the `bls_to_execution_change` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlsToExecutionChangeEvent {
    pub message: BLSToExecutionChange,
    pub signature: BLSSignature,
}

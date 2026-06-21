use alloy_primitives::B256;
use futures::Stream;
use ream_da_errors::DaResult;
use std::pin::Pin;

pub struct HeadEvent {
    pub slot: u64,
    pub block_root: B256,
}

pub struct ReorgEvent {
    pub slot: u64,
    pub depth: u64,
    pub old_head_block: B256,
    pub new_head_block: B256,
}

pub enum ConsensusEvent {
    Head(HeadEvent),
    Reorg(ReorgEvent),
    /// Finalized epoch from FinalizedCheckpointEvent.
    /// Used by the consensus loop to trigger pruning.
    Finalized(u64),
}

pub trait ConsensusClient: Send + Sync {
    fn try_events(&self) -> DaResult<Pin<Box<dyn Stream<Item = DaResult<ConsensusEvent>> + Send>>>;
}

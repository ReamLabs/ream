use ream_consensus_lean::{VoteItem, block::Block};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum LeanChainServiceMessage {
    ProduceBlock(ProduceBlockMessage),
    QueueItem(QueueItem),
}

impl LeanChainServiceMessage {
    pub fn produce_block(slot: u64, response: oneshot::Sender<Block>) -> Self {
        LeanChainServiceMessage::ProduceBlock(ProduceBlockMessage::new(slot, response))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum QueueItem {
    Block(Block),
    Vote(VoteItem),
}

#[derive(Debug)]
pub struct ProduceBlockMessage {
    pub slot: u64,
    pub response: oneshot::Sender<Block>,
}

impl ProduceBlockMessage {
    pub fn new(slot: u64, response: oneshot::Sender<Block>) -> Self {
        ProduceBlockMessage { slot, response }
    }
}

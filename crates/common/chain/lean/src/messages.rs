use ream_consensus_lean::{
    block::{Block, SignedBlock},
    vote::SignedVote,
};
use tokio::sync::oneshot;

/// Messages that exchange information between the [LeanChainService] and other components.
///
/// `ProduceBlock`: Request to produce a new [Block] based on current view of the node.
///
/// `ProcessBlock`: Request to process a new [SignedBlock], with a couple of flags. For flags, see
/// below for the explanation.
///
/// `ProcessVote`: Request to process a new [SignedVote], with a couple of flags. For flags, see
/// below for the explanation.
///
/// Flags:
/// `is_trusted`: If true, the block/vote is considered to 1) be from local or 2) already verified.
/// This flag avoids unnecessary validation of the PQ signature, which can be expensive.
/// `need_gossip`: If true, the block/vote should be gossiped to other peers. In 3SF-mini, a node
/// enqueues an item if it is not ready for processing. The node would later consume the queue
/// (`self.dependencies` in the original Python implementation) for the items. In this case, the
/// node doesn't have to publish block/vote.
#[derive(Debug)]
pub enum LeanChainServiceMessage {
    ProduceBlock {
        slot: u64,
        sender: oneshot::Sender<Block>,
    },
    ProcessBlock {
        signed_block: SignedBlock,
        is_trusted: bool,
        need_gossip: bool,
    },
    ProcessVote {
        signed_vote: SignedVote,
        is_trusted: bool,
        need_gossip: bool,
    },
}

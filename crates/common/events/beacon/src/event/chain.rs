use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use ream_consensus_misc::checkpoint::Checkpoint;
use serde::{Deserialize, Serialize};

/// Head event.
///
/// The node has finished processing, resulting in a new head.
/// `previous_duty_dependent_root` is `get_block_root_at_slot(state,
/// compute_start_slot_at_epoch(epoch - 1) - 1)` and `current_duty_dependent_root` is
/// `get_block_root_at_slot(state, compute_start_slot_at_epoch(epoch) - 1)`. Both dependent roots
/// use the genesis block root in the case of underflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub block: B256,
    pub state: B256,
    pub epoch_transition: bool,
    pub previous_duty_dependent_root: B256,
    pub current_duty_dependent_root: B256,
    pub execution_optimistic: bool,
}

/// Block event.
///
/// The node has received a block (from P2P or API) that is successfully imported
/// on the fork-choice `on_block` handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub block: B256,
    pub execution_optimistic: bool,
}

impl BlockEvent {
    /// Creates a new `BlockEvent` from a signed block.
    ///
    /// `get_checkpoint_block` is a function that computes the checkpoint block for a given epoch
    /// in the chain of the given block root.
    pub fn from_block<F>(
        signed_block: &SignedBeaconBlock,
        finalized_checkpoint: Option<Checkpoint>,
        get_checkpoint_block: F,
    ) -> anyhow::Result<Self>
    where
        F: FnOnce(B256, u64) -> anyhow::Result<B256>,
    {
        let block_root = signed_block.message.block_root();
        let execution_optimistic = match finalized_checkpoint {
            Some(finalized_checkpoint) => {
                // Block is not optimistic (finalized) if it's the finalized checkpoint block itself
                if block_root == finalized_checkpoint.root {
                    false
                } else {
                    let block_epoch =
                        ream_consensus_misc::misc::compute_epoch_at_slot(signed_block.message.slot);
                    let finalized_epoch = finalized_checkpoint.epoch;

                    // If block's epoch is before or equal to finalized epoch, check if it's an
                    // ancestor
                    if block_epoch <= finalized_epoch {
                        match get_checkpoint_block(block_root, finalized_epoch) {
                            Ok(checkpoint_block_at_finalized_epoch) => {
                                // If the checkpoint block at finalized epoch equals the finalized
                                // checkpoint root, this block is an
                                // ancestor of the finalized checkpoint, so it's finalized
                                checkpoint_block_at_finalized_epoch != finalized_checkpoint.root
                            }
                            Err(_) => true, // If we can't determine, assume optimistic
                        }
                    } else {
                        // Block is after finalized epoch, so it's optimistic
                        true
                    }
                }
            }
            None => true, // If no finalized checkpoint, assume optimistic
        };

        Ok(Self {
            slot: signed_block.message.slot,
            block: block_root,
            execution_optimistic,
        })
    }
}

/// Finalized checkpoint event.
///
/// Emitted when the finalized checkpoint has been updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizedCheckpointEvent {
    pub block: B256,
    pub state: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub epoch: u64,
    pub execution_optimistic: bool,
}

/// Chain reorg event.
///
/// Emitted when the chain has been reorganized, resulting in a different canonical head.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainReorgEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub depth: u64,
    pub old_head_block: B256,
    pub new_head_block: B256,
    pub old_head_state: B256,
    pub new_head_state: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub epoch: u64,
    pub execution_optimistic: bool,
}

/// Block gossip event.
///
/// The node has received a block (from P2P or API) that passes validation rules
/// of the beacon_block topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockGossipEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub block: B256,
}

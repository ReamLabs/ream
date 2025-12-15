use libp2p_identity::PeerId;
use ream_consensus_lean::{
    attestation::{AttestationData, SignedAttestation},
    block::{BlockWithSignatures, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
};
use tokio::sync::oneshot;

/// Messages that exchange information between the [LeanChainService] and other components.
///
/// `ProduceBlock`: Request to produce a new [Block] based on current view of the node.
///
/// `BuildAttestationData`: Request to build an [AttestationData] for a given slot.
///
/// `ProcessBlock`: Request to process a new [SignedBlock], with a couple of flags. For flags, see
/// below for the explanation.
///
/// `ProcessAttestation`: Request to process a new [SignedAttestation], with a couple of flags. For
/// flags, see below for the explanation.
///
/// Flags:
/// `need_gossip`: If true, the block/vote should be gossiped to other peers. In 3SF-mini, a node
/// enqueues an item if it is not ready for processing. The node would later consume the queue
/// (`self.dependencies` in the original Python implementation) for the items. In this case, the
/// node doesn't have to publish block/vote.
#[derive(Debug)]
pub enum LeanChainServiceMessage {
    ProduceBlock {
        slot: u64,
        sender: oneshot::Sender<BlockWithSignatures>,
    },
    BuildAttestationData {
        slot: u64,
        sender: oneshot::Sender<AttestationData>,
    },
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
}

use std::str::FromStr;

use alloy_primitives::B256;
use anyhow::anyhow;
// Re-export for compatibility
#[cfg(feature = "eventsource-client")]
pub use eventsource_client::Event;
use ream_bls::BLSSignature;
use ream_consensus_beacon::{
    bls_to_execution_change::BLSToExecutionChange, contribution_and_proof::ContributionAndProof,
    electra::beacon_block::SignedBeaconBlock,
    polynomial_commitments::kzg_commitment::KZGCommitment, voluntary_exit::VoluntaryExit,
};
use ream_consensus_misc::{
    beacon_block_header::SignedBeaconBlockHeader, checkpoint::Checkpoint,
    indexed_attestation::IndexedAttestation,
};
use ream_light_client::{
    finality_update::LightClientFinalityUpdate, optimistic_update::LightClientOptimisticUpdate,
};
use serde::{
    Deserialize, Serialize,
    de::{DeserializeOwned, Error},
};

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

/// Voluntary exit event.
///
/// The node has received a SignedVoluntaryExit (from P2P or API) that passes
/// validation rules of the `voluntary_exit` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoluntaryExitEvent {
    pub message: VoluntaryExit,
    pub signature: BLSSignature,
}

/// Payload attributes event.
///
/// The node has computed new payload attributes for execution payload building.
///
/// This event gives block builders and relays sufficient information to construct or verify
/// a block at `proposal_slot`. The meanings of the fields are:
///
/// - `version`: the identifier of the beacon hard fork at `proposal_slot`, e.g. "bellatrix",
///   "capella".
/// - `proposal_slot`: the slot at which a block using these payload attributes may be built.
/// - `parent_block_root`: the beacon block root of the parent block to be built upon.
/// - `parent_block_number`: the execution block number of the parent block.
/// - `parent_block_hash`: the execution block hash of the parent block.
/// - `proposer_index`: the validator index of the proposer at `proposal_slot` on the chain
///   identified by `parent_block_root`.
/// - `payload_attributes`: beacon API encoding of PayloadAttributesV<N> as defined by the
///   execution-apis specification.
///
/// The frequency at which this event is sent may depend on beacon node configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadAttributesEvent {
    pub version: String,
    pub data: serde_json::Value, // TODO: Properly type this
}

/// Blob sidecar event.
///
/// The node has received a BlobSidecar (from P2P or API) that passes all gossip
/// validations on the `blob_sidecar_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobSidecarEvent {
    pub block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub kzg_commitment: KZGCommitment,
    pub versioned_hash: B256,
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

/// Proposer slashing event.
///
/// The node has received a ProposerSlashing (from P2P or API) that passes
/// validation rules of the `proposer_slashing` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposerSlashingEvent {
    pub signed_header_1: SignedBeaconBlockHeader, // TODO: Properly type this
    pub signed_header_2: SignedBeaconBlockHeader, // TODO: Properly type this
}

/// Attester slashing event.
///
/// The node has received an AttesterSlashing (from P2P or API) that passes
/// validation rules of the `attester_slashing` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttesterSlashingEvent {
    pub attestation_1: IndexedAttestation,
    pub attestation_2: IndexedAttestation,
}

/// Light client finality update event.
///
/// The node's latest known LightClientFinalityUpdate has been updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientFinalityUpdateEvent {
    pub version: String,
    pub data: LightClientFinalityUpdate,
}

/// Light client optimistic update event.
///
/// The node's latest known LightClientOptimisticUpdate has been updated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightClientOptimisticUpdateEvent {
    pub version: String,
    pub data: LightClientOptimisticUpdate,
}

/// Contribution and proof event.
///
/// The node has received a SignedContributionAndProof (from P2P or API) that passes
/// validation rules of the `sync_committee_contribution_and_proof` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAndProofEvent {
    pub message: ContributionAndProof,
    pub signature: BLSSignature,
}

/// Attestation event.
///
/// The node has received an Attestation (from P2P or API) that passes validation
/// rules of the `beacon_attestation_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationEvent {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    pub beacon_block_root: B256,
    pub source: serde_json::Value, // TODO: Properly type this
    pub target: serde_json::Value, // TODO: Properly type this
}

/// Data column sidecar event.
///
/// The node has received a DataColumnSidecar (from P2P or API) that passes all gossip
/// validations on the `data_column_sidecar_{subnet_id}` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataColumnSidecarEvent {
    pub block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub kzg_commitments: Vec<KZGCommitment>,
}

/// Event topic enum for filtering events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTopic {
    ChainReorg,
    VoluntaryExit,
    PayloadAttributes,
    BlobSidecar,
    Block,
    BlsToExecutionChange,
    Head,
    LightClientFinalityUpdate,
    LightClientOptimisticUpdate,
    ContributionAndProof,
    FinalizedCheckpoint,
    Attestation,
    ProposerSlashing,
    AttesterSlashing,
    DataColumnSidecar,
}

impl FromStr for EventTopic {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "chain_reorg" => EventTopic::ChainReorg,
            "voluntary_exit" => EventTopic::VoluntaryExit,
            "payload_attributes" => EventTopic::PayloadAttributes,
            "blob_sidecar" => EventTopic::BlobSidecar,
            "block" => EventTopic::Block,
            "bls_to_execution_change" => EventTopic::BlsToExecutionChange,
            "head" => EventTopic::Head,
            "light_client_finality_update" => EventTopic::LightClientFinalityUpdate,
            "light_client_optimistic_update" => EventTopic::LightClientOptimisticUpdate,
            "contribution_and_proof" => EventTopic::ContributionAndProof,
            "finalized_checkpoint" => EventTopic::FinalizedCheckpoint,
            "attestation" => EventTopic::Attestation,
            "proposer_slashing" => EventTopic::ProposerSlashing,
            "attester_slashing" => EventTopic::AttesterSlashing,
            "data_column_sidecar" => EventTopic::DataColumnSidecar,
            _ => return Err(anyhow!("Invalid Event Topic: {s}")),
        })
    }
}

impl std::fmt::Display for EventTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                EventTopic::ChainReorg => "chain_reorg",
                EventTopic::VoluntaryExit => "voluntary_exit",
                EventTopic::PayloadAttributes => "payload_attributes",
                EventTopic::BlobSidecar => "blob_sidecar",
                EventTopic::Block => "block",
                EventTopic::BlsToExecutionChange => "bls_to_execution_change",
                EventTopic::Head => "head",
                EventTopic::LightClientFinalityUpdate => "light_client_finality_update",
                EventTopic::LightClientOptimisticUpdate => "light_client_optimistic_update",
                EventTopic::ContributionAndProof => "contribution_and_proof",
                EventTopic::FinalizedCheckpoint => "finalized_checkpoint",
                EventTopic::Attestation => "attestation",
                EventTopic::ProposerSlashing => "proposer_slashing",
                EventTopic::AttesterSlashing => "attester_slashing",
                EventTopic::DataColumnSidecar => "data_column_sidecar",
            }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum BeaconEvent {
    Head(HeadEvent),
    Block(BlockEvent),
    FinalizedCheckpoint(FinalizedCheckpointEvent),
    ChainReorg(ChainReorgEvent),
    VoluntaryExit(VoluntaryExitEvent),
    PayloadAttributes(PayloadAttributesEvent),
    BlobSidecar(BlobSidecarEvent),
    BlsToExecutionChange(BlsToExecutionChangeEvent),
    LightClientFinalityUpdate(Box<LightClientFinalityUpdateEvent>),
    LightClientOptimisticUpdate(Box<LightClientOptimisticUpdateEvent>),
    ContributionAndProof(ContributionAndProofEvent),
    Attestation(AttestationEvent),
    ProposerSlashing(ProposerSlashingEvent),
    AttesterSlashing(AttesterSlashingEvent),
    DataColumnSidecar(DataColumnSidecarEvent),
}

impl BeaconEvent {
    fn from_json<T: DeserializeOwned>(
        json: &str,
        constructor: impl FnOnce(T) -> Self,
    ) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json).map(constructor)
    }

    /// Returns the event name as a string (e.g., "head", "block", "finalized_checkpoint").
    pub fn event_name(&self) -> &'static str {
        match self {
            BeaconEvent::Head(_) => "head",
            BeaconEvent::Block(_) => "block",
            BeaconEvent::FinalizedCheckpoint(_) => "finalized_checkpoint",
            BeaconEvent::ChainReorg(_) => "chain_reorg",
            BeaconEvent::VoluntaryExit(_) => "voluntary_exit",
            BeaconEvent::PayloadAttributes(_) => "payload_attributes",
            BeaconEvent::BlobSidecar(_) => "blob_sidecar",
            BeaconEvent::BlsToExecutionChange(_) => "bls_to_execution_change",
            BeaconEvent::LightClientFinalityUpdate(_) => "light_client_finality_update",
            BeaconEvent::LightClientOptimisticUpdate(_) => "light_client_optimistic_update",
            BeaconEvent::ContributionAndProof(_) => "contribution_and_proof",
            BeaconEvent::Attestation(_) => "attestation",
            BeaconEvent::ProposerSlashing(_) => "proposer_slashing",
            BeaconEvent::AttesterSlashing(_) => "attester_slashing",
            BeaconEvent::DataColumnSidecar(_) => "data_column_sidecar",
        }
    }

    /// Serializes only the event data (without the enum wrapper).
    /// This is used for SSE where we send `event: <name>` and `data: <json>` separately.
    pub fn serialize_data(&self) -> Result<String, serde_json::Error> {
        match self {
            BeaconEvent::Head(data) => serde_json::to_string(data),
            BeaconEvent::Block(data) => serde_json::to_string(data),
            BeaconEvent::FinalizedCheckpoint(data) => serde_json::to_string(data),
            BeaconEvent::ChainReorg(data) => serde_json::to_string(data),
            BeaconEvent::VoluntaryExit(data) => serde_json::to_string(data),
            BeaconEvent::PayloadAttributes(data) => serde_json::to_string(data),
            BeaconEvent::BlobSidecar(data) => serde_json::to_string(data),
            BeaconEvent::BlsToExecutionChange(data) => serde_json::to_string(data),
            BeaconEvent::LightClientFinalityUpdate(data) => serde_json::to_string(data),
            BeaconEvent::LightClientOptimisticUpdate(data) => serde_json::to_string(data),
            BeaconEvent::ContributionAndProof(data) => serde_json::to_string(data),
            BeaconEvent::Attestation(data) => serde_json::to_string(data),
            BeaconEvent::ProposerSlashing(data) => serde_json::to_string(data),
            BeaconEvent::AttesterSlashing(data) => serde_json::to_string(data),
            BeaconEvent::DataColumnSidecar(data) => serde_json::to_string(data),
        }
    }
}

#[cfg(feature = "eventsource-client")]
impl TryFrom<Event> for BeaconEvent {
    type Error = serde_json::Error;

    fn try_from(event: Event) -> Result<Self, Self::Error> {
        let event_type =
            EventTopic::from_str(event.event_type.as_str()).map_err(Self::Error::custom)?;
        match event_type {
            EventTopic::ChainReorg => Self::from_json(event.data.as_str(), Self::ChainReorg),
            EventTopic::VoluntaryExit => Self::from_json(event.data.as_str(), Self::VoluntaryExit),
            EventTopic::PayloadAttributes => {
                Self::from_json(event.data.as_str(), Self::PayloadAttributes)
            }
            EventTopic::BlobSidecar => Self::from_json(event.data.as_str(), Self::BlobSidecar),
            EventTopic::Block => Self::from_json(event.data.as_str(), Self::Block),
            EventTopic::BlsToExecutionChange => {
                Self::from_json(event.data.as_str(), Self::BlsToExecutionChange)
            }
            EventTopic::Head => Self::from_json(event.data.as_str(), Self::Head),
            EventTopic::LightClientFinalityUpdate => {
                Self::from_json(event.data.as_str(), Self::LightClientFinalityUpdate)
            }
            EventTopic::LightClientOptimisticUpdate => {
                Self::from_json(event.data.as_str(), Self::LightClientOptimisticUpdate)
            }
            EventTopic::ContributionAndProof => {
                Self::from_json(event.data.as_str(), Self::ContributionAndProof)
            }
            EventTopic::FinalizedCheckpoint => {
                Self::from_json(event.data.as_str(), Self::FinalizedCheckpoint)
            }
            EventTopic::Attestation => Self::from_json(event.data.as_str(), Self::Attestation),
            EventTopic::ProposerSlashing => {
                Self::from_json(event.data.as_str(), Self::ProposerSlashing)
            }
            EventTopic::AttesterSlashing => {
                Self::from_json(event.data.as_str(), Self::AttesterSlashing)
            }
            EventTopic::DataColumnSidecar => {
                Self::from_json(event.data.as_str(), Self::DataColumnSidecar)
            }
        }
    }
}

/// Trait for sending beacon events.
///
/// This trait provides a convenient way to send events through an optional broadcast sender,
/// handling the None case and errors gracefully.
pub trait BeaconEventSender {
    /// Send an event if the sender is available.
    ///
    /// Returns silently if the sender is None or if sending fails (logs a warning).
    fn send_event(&self, event: BeaconEvent);
}

impl BeaconEventSender for Option<tokio::sync::broadcast::Sender<BeaconEvent>> {
    fn send_event(&self, event: BeaconEvent) {
        let Some(sender) = self.as_ref() else {
            return;
        };

        let event_name = event.event_name();
        if let Err(e) = sender.send(event) {
            tracing::warn!("Failed to send {} event: {}", event_name, e);
        }
    }
}

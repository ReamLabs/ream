pub mod contribution_and_proof;
pub mod event;

use std::str::FromStr;

use anyhow::anyhow;
use eventsource_client::Event;
use serde::{
    Deserialize, Serialize,
    de::{DeserializeOwned, Error},
};

use crate::event::{
    attestation::{AttestationEvent, SingleAttestationEvent},
    blob::{BlobSidecarEvent, DataColumnSidecarEvent},
    chain::{BlockEvent, BlockGossipEvent, ChainReorgEvent, FinalizedCheckpointEvent, HeadEvent},
    execution::PayloadAttributesEvent,
    light_client::{LightClientFinalityUpdateEvent, LightClientOptimisticUpdateEvent},
    slashing::{AttesterSlashingEvent, ProposerSlashingEvent},
    sync_committee::ContributionAndProofEvent,
    validator::{BlsToExecutionChangeEvent, VoluntaryExitEvent},
};

/// Event topic enum for filtering events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTopic {
    ChainReorg,
    VoluntaryExit,
    PayloadAttributes,
    BlobSidecar,
    Block,
    BlockGossip,
    BlsToExecutionChange,
    Head,
    LightClientFinalityUpdate,
    LightClientOptimisticUpdate,
    ContributionAndProof,
    FinalizedCheckpoint,
    SingleAttestation,
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
            "block_gossip" => EventTopic::BlockGossip,
            "bls_to_execution_change" => EventTopic::BlsToExecutionChange,
            "head" => EventTopic::Head,
            "light_client_finality_update" => EventTopic::LightClientFinalityUpdate,
            "light_client_optimistic_update" => EventTopic::LightClientOptimisticUpdate,
            "contribution_and_proof" => EventTopic::ContributionAndProof,
            "finalized_checkpoint" => EventTopic::FinalizedCheckpoint,
            "single_attestation" => EventTopic::SingleAttestation,
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
                EventTopic::BlockGossip => "block_gossip",
                EventTopic::BlsToExecutionChange => "bls_to_execution_change",
                EventTopic::Head => "head",
                EventTopic::LightClientFinalityUpdate => "light_client_finality_update",
                EventTopic::LightClientOptimisticUpdate => "light_client_optimistic_update",
                EventTopic::ContributionAndProof => "contribution_and_proof",
                EventTopic::FinalizedCheckpoint => "finalized_checkpoint",
                EventTopic::SingleAttestation => "single_attestation",
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
    BlockGossip(BlockGossipEvent),
    SingleAttestation(SingleAttestationEvent),
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
            BeaconEvent::BlockGossip(_) => "block_gossip",
            BeaconEvent::SingleAttestation(_) => "single_attestation",
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
            BeaconEvent::BlockGossip(data) => serde_json::to_string(data),
            BeaconEvent::SingleAttestation(data) => serde_json::to_string(data),
        }
    }
}

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
            EventTopic::BlockGossip => Self::from_json(event.data.as_str(), Self::BlockGossip),
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
            EventTopic::SingleAttestation => {
                Self::from_json(event.data.as_str(), Self::SingleAttestation)
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
        if let Err(err) = sender.send(event) {
            tracing::warn!("Failed to send {} event: {}", event_name, err);
        }
    }
}

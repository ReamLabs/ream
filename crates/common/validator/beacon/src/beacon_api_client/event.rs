use std::{fmt::Display, str::FromStr};

use alloy_rpc_types_beacon::events::{
    AttestationEvent, BlobSidecarEvent, BlockEvent, BlsToExecutionChangeEvent, ChainReorgEvent,
    ContributionAndProofEvent, FinalizedCheckpointEvent, HeadEvent, LightClientFinalityUpdateEvent,
    LightClientOptimisticUpdateEvent, PayloadAttributesEvent, VoluntaryExitEvent,
};
use anyhow::anyhow;
use eventsource_client::Event;
use serde::de::{DeserializeOwned, Error};

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
            _ => return Err(anyhow!("Invalid Event Topic: {s}")),
        })
    }
}

impl Display for EventTopic {
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
            }
        )
    }
}

pub enum BeaconEvent {
    ChainReorg(ChainReorgEvent),
    VoluntaryExit(VoluntaryExitEvent),
    PayloadAttributes(PayloadAttributesEvent),
    BlobSidecar(BlobSidecarEvent),
    Block(BlockEvent),
    BlsToExecutionChange(BlsToExecutionChangeEvent),
    Head(HeadEvent),
    LightClientFinalityUpdate(LightClientFinalityUpdateEvent),
    LightClientOptimisticUpdate(LightClientOptimisticUpdateEvent),
    ContributionAndProof(ContributionAndProofEvent),
    FinalizedCheckpoint(FinalizedCheckpointEvent),
    Attestation(AttestationEvent),
}

impl BeaconEvent {
    fn from_json<T: DeserializeOwned>(
        json: &str,
        constructor: impl FnOnce(T) -> Self,
    ) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json).map(constructor)
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
        }
    }
}

use alloy_rpc_types_beacon::events::{
    AttestationEvent, BeaconNodeEventTopic, BlobSidecarEvent, BlockEvent,
    BlsToExecutionChangeEvent, ChainReorgEvent, ContributionAndProofEvent,
    FinalizedCheckpointEvent, HeadEvent, LightClientFinalityUpdateEvent,
    LightClientOptimisticUpdateEvent, PayloadAttributesEvent, VoluntaryExitEvent,
};
use eventsource_client::Event;
use serde::de::{DeserializeOwned, Error};

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
        if event.event_type == BeaconNodeEventTopic::ChainReorg.query_value() {
            Self::from_json(&event.data, Self::ChainReorg)
        } else if event.event_type == BeaconNodeEventTopic::VoluntaryExit.query_value() {
            Self::from_json(&event.data, Self::VoluntaryExit)
        } else if event.event_type == BeaconNodeEventTopic::PayloadAttributes.query_value() {
            Self::from_json(&event.data, Self::PayloadAttributes)
        } else if event.event_type == BeaconNodeEventTopic::BlobSidecar.query_value() {
            Self::from_json(&event.data, Self::BlobSidecar)
        } else if event.event_type == BeaconNodeEventTopic::Block.query_value() {
            Self::from_json(&event.data, Self::Block)
        } else if event.event_type == BeaconNodeEventTopic::BlsToExecutionChange.query_value() {
            Self::from_json(&event.data, Self::BlsToExecutionChange)
        } else if event.event_type == BeaconNodeEventTopic::Head.query_value() {
            Self::from_json(&event.data, Self::Head)
        } else if event.event_type == BeaconNodeEventTopic::LightClientFinalityUpdate.query_value()
        {
            Self::from_json(&event.data, Self::LightClientFinalityUpdate)
        } else if event.event_type
            == BeaconNodeEventTopic::LightClientOptimisticUpdate.query_value()
        {
            Self::from_json(&event.data, Self::LightClientOptimisticUpdate)
        } else if event.event_type == BeaconNodeEventTopic::ContributionAndProof.query_value() {
            Self::from_json(&event.data, Self::ContributionAndProof)
        } else if event.event_type == BeaconNodeEventTopic::FinalizedCheckpoint.query_value() {
            Self::from_json(&event.data, Self::FinalizedCheckpoint)
        } else if event.event_type == BeaconNodeEventTopic::Attestation.query_value() {
            Self::from_json(&event.data, Self::Attestation)
        } else {
            Err(Self::Error::custom(format!(
                "Can't create BeaconEvent: unexpected event type: {}",
                event.event_type,
            )))
        }
    }
}

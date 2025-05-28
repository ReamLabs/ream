use alloy_rpc_types_beacon::events::{BeaconNodeEventTopic, ChainReorgEvent, VoluntaryExitEvent};
use eventsource_client::Event;
use serde::de::{DeserializeOwned, Error};

pub enum BeaconEvent {
    ChainReorg(ChainReorgEvent),
    VoluntaryExit(VoluntaryExitEvent),
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
        } else {
            Err(Self::Error::custom(format!(
                "Can't create BeaconEvent: unexpected event type: {}",
                event.event_type,
            )))
        }
    }
}

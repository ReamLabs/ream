use std::{pin::Pin, time::Duration};

use futures::{Stream, StreamExt};
use ream_da_errors::{DaError, DaResult};
use ream_events_beacon::{BeaconEvent, EventTopic};
use ream_validator_beacon::beacon_api_client::BeaconApiClient;
use reqwest::Url;

use crate::iface::{ConsensusClient, ConsensusEvent, HeadEvent, ReorgEvent};

pub struct DaConsensusClient {
    inner: BeaconApiClient,
}

impl DaConsensusClient {
    pub fn new(beacon_url: Url) -> DaResult<Self> {
        Ok(Self {
            inner: BeaconApiClient::new(beacon_url, Duration::from_secs(30))?,
        })
    }
}

impl ConsensusClient for DaConsensusClient {
    fn try_events(&self) -> DaResult<Pin<Box<dyn Stream<Item = DaResult<ConsensusEvent>> + Send>>> {
        let stream = self
            .inner
            .get_events_stream(
                &[
                    EventTopic::Head,
                    EventTopic::ChainReorg,
                    EventTopic::FinalizedCheckpoint,
                ],
                "da-node",
            )
            .map_err(|e| DaError::EventStreamFailed(e.to_string()))?;

        let mapped_stream = stream
            .filter_map(|event| async move {
                match event {
                    BeaconEvent::Head(h) => Some(Ok(ConsensusEvent::Head(HeadEvent {
                        slot: h.slot,
                        block_root: h.block,
                    }))),

                    BeaconEvent::ChainReorg(r) => Some(Ok(ConsensusEvent::Reorg(ReorgEvent {
                        slot: r.slot,
                        depth: r.depth,
                        old_head_block: r.old_head_block,
                        new_head_block: r.new_head_block,
                    }))),

                    BeaconEvent::FinalizedCheckpoint(f) => {
                        Some(Ok(ConsensusEvent::Finalized(f.epoch)))
                    }
                    _ => None,
                }
            })
            .boxed();

        Ok(mapped_stream)
    }
}

use alloy_primitives::B256;
use anyhow::bail;
use libp2p::PeerId;
use ream_consensus_beacon::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    electra::beacon_block::SignedBeaconBlock,
};
use ream_executor::ReamExecutor;
use ream_p2p::{
    channel::{P2PCallbackResponse, P2PMessage, P2PRequest},
    req_resp::messages::ResponseMessage,
};
use ssz::Encode;
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
};
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start_slot: u64,
    pub count: u64,
}

impl Range {
    pub fn new(start_slot: u64, count: u64) -> Self {
        Self { start_slot, count }
    }
}

pub struct PeerRangeDownloader;

impl PeerRangeDownloader {
    pub fn start(
        peer_id: PeerId,
        p2p_sender: UnboundedSender<P2PMessage>,
        executor: ReamExecutor,
        range: Range,
    ) -> JoinHandle<anyhow::Result<anyhow::Result<Vec<SignedBeaconBlock>>>> {
        executor.spawn(async move {
            let mut beacon_blocks = vec![];
            let (callback, mut rx) = mpsc::channel(100);
            p2p_sender
                .send(P2PMessage::Request(P2PRequest::BlockRange {
                    peer_id,
                    start: range.start_slot,
                    count: range.count,
                    callback,
                }))
                .expect("Failed to send block range request");

            while let Some(response) = rx.recv().await {
                match response {
                    Ok(P2PCallbackResponse::ResponseMessage(message)) => {
                        if let ResponseMessage::BeaconBlocksByRange(blocks) = *message {
                            info!(
                                "Received block response with slot {} length {}",
                                blocks.message.slot,
                                blocks.as_ssz_bytes().len()
                            );
                            beacon_blocks.push(blocks);
                        }
                    }
                    Ok(P2PCallbackResponse::EndOfStream) => {
                        info!("End of block range request stream received.");
                        break;
                    }
                    Ok(P2PCallbackResponse::Disconnected) => {
                        bail!("Peer disconnected while receiving block range.");
                    }
                    Ok(P2PCallbackResponse::Timeout) => {
                        bail!("Block range request timed out.");
                    }
                    Err(err) => {
                        info!("Error receiving BeaconBlocks from block range request: {err:?}");
                    }
                }
            }

            Ok(beacon_blocks)
        })
    }
}

pub struct PeerRootsDownloader;

impl PeerRootsDownloader {
    pub fn start(
        peer_id: PeerId,
        p2p_sender: UnboundedSender<P2PMessage>,
        executor: ReamExecutor,
        roots: Vec<B256>,
    ) -> JoinHandle<anyhow::Result<anyhow::Result<Vec<SignedBeaconBlock>>>> {
        executor.spawn(async move {
            let mut beacon_blocks = vec![];
            let (callback, mut rx) = mpsc::channel(100);
            p2p_sender
                .send(P2PMessage::Request(P2PRequest::BlockRoots {
                    peer_id,
                    roots: roots.to_vec(),
                    callback,
                }))
                .expect("Failed to send block roots request");

            while let Some(response) = rx.recv().await {
                match response {
                    Ok(P2PCallbackResponse::ResponseMessage(message)) => {
                        if let ResponseMessage::BeaconBlocksByRoot(blocks) = *message {
                            info!(
                                "Received block response with slot {} length {}",
                                blocks.message.slot,
                                blocks.as_ssz_bytes().len()
                            );
                            beacon_blocks.push(blocks);
                        }
                    }
                    Ok(P2PCallbackResponse::EndOfStream) => {
                        info!("End of block roots request stream received.");
                        break;
                    }
                    Ok(P2PCallbackResponse::Disconnected) => {
                        bail!("Peer disconnected while receiving block roots.");
                    }
                    Ok(P2PCallbackResponse::Timeout) => {
                        bail!("Block roots request timed out.");
                    }
                    Err(err) => {
                        info!("Error receiving BeaconBlocks from block roots request: {err:?}");
                    }
                }
            }

            Ok(beacon_blocks)
        })
    }
}

pub struct PeerBlobIdentifierDownloader;

impl PeerBlobIdentifierDownloader {
    pub fn start(
        peer_id: PeerId,
        p2p_sender: UnboundedSender<P2PMessage>,
        executor: ReamExecutor,
        blob_identifiers: Vec<BlobIdentifier>,
    ) -> JoinHandle<anyhow::Result<anyhow::Result<Vec<BlobSidecar>>>> {
        executor.spawn(async move {
            let mut blob_sidecars = vec![];
            let (callback, mut rx) = mpsc::channel(100);
            p2p_sender
                .send(P2PMessage::Request(P2PRequest::BlobIdentifiers {
                    peer_id,
                    blob_identifiers: blob_identifiers.to_vec(),
                    callback,
                }))
                .expect("Failed to send blob identifiers request");

            while let Some(response) = rx.recv().await {
                match response {
                    Ok(P2PCallbackResponse::ResponseMessage(message)) => {
                        if let ResponseMessage::BlobSidecarsByRoot(blob_sidecar) = *message {
                            info!(
                                "Received blob sidecar response with index {} length {}",
                                blob_sidecar.index,
                                blob_sidecar.as_ssz_bytes().len()
                            );
                            blob_sidecars.push(blob_sidecar);
                        }
                    }
                    Ok(P2PCallbackResponse::EndOfStream) => {
                        info!("End of blob roots request stream received.");
                        break;
                    }
                    Ok(P2PCallbackResponse::Disconnected) => {
                        bail!("Peer disconnected while receiving blob sidecars.");
                    }
                    Ok(P2PCallbackResponse::Timeout) => {
                        bail!("Blob identifiers request timed out.");
                    }
                    Err(err) => {
                        info!("Error receiving blobs from blob roots request: {err:?}");
                    }
                }
            }

            Ok(blob_sidecars)
        })
    }
}

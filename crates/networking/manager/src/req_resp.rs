use std::sync::Arc;

use libp2p::{PeerId, swarm::ConnectionId};
use ream_consensus_beacon::blob_sidecar::BlobIdentifier;
use ream_p2p::{
    network_state::NetworkState,
    req_resp::messages::{
        RequestMessage, ResponseMessage,
        beacon_blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request},
        blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request},
    },
};
use ream_storage::{db::ReamDB, tables::Table};
use tracing::{info, trace, warn};

use crate::p2p_sender::P2PSender;

pub async fn handle_req_resp_message(
    peer_id: PeerId,
    stream_id: u64,
    connection_id: ConnectionId,
    message: RequestMessage,
    p2p_sender: &P2PSender,
    ream_db: &ReamDB,
    network_state: Arc<NetworkState>,
) {
    match message {
        RequestMessage::Status(status) => {
            trace!(
                ?peer_id,
                ?stream_id,
                ?connection_id,
                ?status,
                "Received Status request"
            );

            p2p_sender.send_response(
                peer_id,
                connection_id,
                stream_id,
                ResponseMessage::Status(network_state.status.read().clone()),
            );

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        RequestMessage::BeaconBlocksByRange(BeaconBlocksByRangeV2Request {
            start_slot,
            count,
            ..
        }) => {
            for slot in start_slot..start_slot + count {
                let Ok(Some(block_root)) = ream_db.slot_index_provider().get(slot) else {
                    trace!("No block root found for slot {slot}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block root found for slot {slot}"),
                    );
                    return;
                };
                let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                    trace!("No block found for root {block_root}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block found for root {block_root}"),
                    );
                    return;
                };

                p2p_sender.send_response(
                    peer_id,
                    connection_id,
                    stream_id,
                    ResponseMessage::BeaconBlocksByRange(block),
                );
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        RequestMessage::BeaconBlocksByRoot(BeaconBlocksByRootV2Request { inner }) => {
            for block_root in inner {
                let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                    trace!("No block found for root {block_root}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block found for root {block_root}"),
                    );
                    return;
                };

                p2p_sender.send_response(
                    peer_id,
                    connection_id,
                    stream_id,
                    ResponseMessage::BeaconBlocksByRoot(block),
                );
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        RequestMessage::BlobSidecarsByRange(BlobSidecarsByRangeV1Request { start_slot, count }) => {
            for slot in start_slot..start_slot + count {
                let Ok(Some(block_root)) = ream_db.slot_index_provider().get(slot) else {
                    trace!("No block root found for slot {slot}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block root found for slot {slot}"),
                    );
                    return;
                };
                let Ok(Some(block)) = ream_db.beacon_block_provider().get(block_root) else {
                    trace!("No block found for root {block_root}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block found for root {block_root}"),
                    );
                    return;
                };

                for index in 0..block.message.body.blob_kzg_commitments.len() {
                    let Ok(Some(blob_and_proof)) = ream_db
                        .blobs_and_proofs_provider()
                        .get(BlobIdentifier::new(block_root, index as u64))
                    else {
                        trace!(
                            "No blob and proof found for block root {block_root} and index {index}"
                        );
                        p2p_sender.send_error_response(
                            peer_id,
                            connection_id,
                            stream_id,
                            &format!("No blob and proof found for block root {block_root} and index {index}"),
                        );
                        return;
                    };

                    let blob_sidecar = match block.blob_sidecar(blob_and_proof, index as u64) {
                        Ok(blob_sidecar) => blob_sidecar,
                        Err(err) => {
                            info!(
                                "Failed to create blob sidecar for block root {block_root} and index {index}: {err}"
                            );
                            p2p_sender.send_error_response(
                                peer_id,
                                connection_id,
                                stream_id,
                                &format!("Failed to create blob sidecar: {err}"),
                            );
                            return;
                        }
                    };

                    p2p_sender.send_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        ResponseMessage::BlobSidecarsByRange(blob_sidecar),
                    );
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        RequestMessage::BlobSidecarsByRoot(BlobSidecarsByRootV1Request { inner }) => {
            for blob_identifier in inner {
                let Ok(Some(blob_and_proof)) =
                    ream_db.blobs_and_proofs_provider().get(blob_identifier)
                else {
                    trace!("No blob and proof found for identifier {blob_identifier:?}");
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No blob and proof found for identifier {blob_identifier:?}"),
                    );
                    return;
                };

                let Ok(Some(block)) = ream_db
                    .beacon_block_provider()
                    .get(blob_identifier.block_root)
                else {
                    trace!("No block found for root {}", blob_identifier.block_root);
                    p2p_sender.send_error_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        &format!("No block found for root {}", blob_identifier.block_root),
                    );
                    return;
                };

                let blob_sidecar = match block.blob_sidecar(blob_and_proof, blob_identifier.index) {
                    Ok(blob_sidecar) => blob_sidecar,
                    Err(err) => {
                        info!(
                            "Failed to create blob sidecar for identifier {blob_identifier:?}: {err}"
                        );
                        p2p_sender.send_error_response(
                            peer_id,
                            connection_id,
                            stream_id,
                            &format!("Failed to create blob sidecar: {err}"),
                        );
                        return;
                    }
                };

                p2p_sender.send_response(
                    peer_id,
                    connection_id,
                    stream_id,
                    ResponseMessage::BlobSidecarsByRoot(blob_sidecar),
                );
            }
            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        _ => warn!("This message shouldn't be handled in the network manager: {message:?}"),
    };
}

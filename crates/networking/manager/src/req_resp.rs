use std::sync::Arc;

use libp2p::{PeerId, swarm::ConnectionId};
use ream_consensus_beacon::{
    blob_sidecar::BlobIdentifier, data_column_sidecar::ColumnIdentifier,
};
use ream_p2p::{
    network::beacon::network_state::NetworkState,
    req_resp::beacon::messages::{
        BeaconRequestMessage, BeaconResponseMessage,
        blob_sidecars::{BlobSidecarsByRangeV1Request, BlobSidecarsByRootV1Request},
        blocks::{BeaconBlocksByRangeV2Request, BeaconBlocksByRootV2Request},
        data_column_sidecars::{DataColumnSidecarsByRangeV1Request, DataColumnSidecarsByRootV1Request},
    },
};
use ream_storage::{
    db::beacon::BeaconDB,
    tables::table::{CustomTable, REDBTable},
};
use tracing::{info, trace, warn};

use crate::p2p_sender::P2PSender;

pub async fn handle_req_resp_message(
    peer_id: PeerId,
    stream_id: u64,
    connection_id: ConnectionId,
    message: BeaconRequestMessage,
    p2p_sender: &P2PSender,
    ream_db: &BeaconDB,
    network_state: Arc<NetworkState>,
) {
    match message {
        BeaconRequestMessage::Status(status) => {
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
                BeaconResponseMessage::Status(network_state.status.read().clone()),
            );

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::BeaconBlocksByRange(BeaconBlocksByRangeV2Request {
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
                let Ok(Some(block)) = ream_db.block_provider().get(block_root) else {
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
                    BeaconResponseMessage::BeaconBlocksByRange(block),
                );
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::BeaconBlocksByRoot(BeaconBlocksByRootV2Request { inner }) => {
            for block_root in inner {
                let Ok(Some(block)) = ream_db.block_provider().get(block_root) else {
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
                    BeaconResponseMessage::BeaconBlocksByRoot(block),
                );
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::BlobSidecarsByRange(BlobSidecarsByRangeV1Request {
            start_slot,
            count,
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
                let Ok(Some(block)) = ream_db.block_provider().get(block_root) else {
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
                        BeaconResponseMessage::BlobSidecarsByRange(blob_sidecar),
                    );
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::BlobSidecarsByRoot(BlobSidecarsByRootV1Request { inner }) => {
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

                let Ok(Some(block)) = ream_db.block_provider().get(blob_identifier.block_root)
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
                    BeaconResponseMessage::BlobSidecarsByRoot(blob_sidecar),
                );
            }
            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::DataColumnSidecarsByRange(DataColumnSidecarsByRangeV1Request {
            start_slot,
            count,
            columns,
        }) => {
            for slot in start_slot..start_slot + count {
                let Ok(Some(block_root)) = ream_db.slot_index_provider().get(slot) else {
                    trace!("No block root found for slot {slot}");
                    continue;
                };

                for &column_index in &columns {
                    let column_identifier = ColumnIdentifier::new(block_root, column_index);
                    let Ok(Some(column_sidecar)) =
                        ream_db.column_sidecars_provider().get(column_identifier)
                    else {
                        trace!(
                            "No column sidecar found for block root {block_root} and index {column_index}"
                        );
                        continue;
                    };

                    p2p_sender.send_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        BeaconResponseMessage::DataColumnSidecarsByRange(column_sidecar),
                    );
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        BeaconRequestMessage::DataColumnSidecarsByRoot(DataColumnSidecarsByRootV1Request {
            inner,
        }) => {
            for identifier in inner {
                for &column_index in &identifier.columns {
                    let column_identifier = ColumnIdentifier::new(identifier.block_root, column_index);
                    let Ok(Some(column_sidecar)) =
                        ream_db.column_sidecars_provider().get(column_identifier)
                    else {
                        trace!(
                            "No column sidecar found for block root {} and index {column_index}",
                            identifier.block_root
                        );
                        continue;
                    };

                    p2p_sender.send_response(
                        peer_id,
                        connection_id,
                        stream_id,
                        BeaconResponseMessage::DataColumnSidecarsByRoot(column_sidecar),
                    );
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }
        _ => warn!("This message shouldn't be handled in the network manager: {message:?}"),
    };
}

use std::sync::Arc;

use libp2p::{PeerId, swarm::ConnectionId};
use ream_req_resp::beacon::messages::{
    BeaconRequestMessage, BeaconResponseMessage,
    data_column_sidecars::{DataColumnSidecarsByRangeV1Request, DataColumnSidecarsByRootV1Request},
};
use tracing::{trace, warn};

use ream_da_storage::DaStore;
use ream_network_manager::p2p_sender::P2PSender;

pub async fn handle_da_req_resp_message(
    peer_id: PeerId,
    stream_id: u64,
    connection_id: ConnectionId,
    message: BeaconRequestMessage,
    p2p_sender: &P2PSender,
    store: &Arc<DaStore>,
) {
    match message {
        BeaconRequestMessage::DataColumnSidecarsByRange(DataColumnSidecarsByRangeV1Request {
            start_slot,
            count,
            columns,
        }) => {
            match store.slots_in_range(start_slot, count) {
                Ok(slot_roots) => {
                    for (_, block_root) in slot_roots {
                        for &column_index in &columns {
                            match store.get_column_owned(block_root, column_index).await {
                                Ok(Some(sidecar)) => {
                                    p2p_sender.send_response(
                                        peer_id,
                                        connection_id,
                                        stream_id,
                                        BeaconResponseMessage::DataColumnSidecarsByRange(sidecar),
                                    );
                                }
                                Ok(None) => {
                                    trace!(
                                        %block_root,
                                        column = column_index,
                                        "Column not found for range request"
                                    );
                                }
                                Err(err) => {
                                    warn!("Storage error: {err}");
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!("Failed to query slot range: {err}");
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }

        BeaconRequestMessage::DataColumnSidecarsByRoot(DataColumnSidecarsByRootV1Request {
            inner,
        }) => {
            for identifier in inner {
                for &column_index in &identifier.columns {
                    match store
                        .get_column_owned(identifier.block_root, column_index)
                        .await
                    {
                        Ok(Some(sidecar)) => {
                            p2p_sender.send_response(
                                peer_id,
                                connection_id,
                                stream_id,
                                BeaconResponseMessage::DataColumnSidecarsByRoot(sidecar),
                            );
                        }
                        Ok(None) => {
                            trace!(
                                block_root = %identifier.block_root,
                                column = column_index,
                                "Column not found for root request"
                            );
                        }
                        Err(err) => {
                            warn!("Storage error: {err}");
                        }
                    }
                }
            }

            p2p_sender.send_end_of_stream_response(peer_id, connection_id, stream_id);
        }

        other => {
            trace!("DA node ignoring non-column request: {other:?}");
        }
    }
}

use std::sync::Arc;

use alloy_primitives::aliases::B32;
use libp2p::gossipsub::Message;
use ream_consensus_beacon::data_column_sidecar::{
    DATA_COLUMN_SIDECAR_SUBNET_COUNT, NUMBER_OF_COLUMNS,
};
use ream_da_storage::DaStore;
use ream_network_manager::p2p_sender::P2PSender;
use ream_p2p::gossipsub::beacon::{
    configurations::GossipsubConfig,
    message::GossipsubMessage,
    topics::{GossipTopic, GossipTopicKind},
};
use ream_polynomial_commitments::handlers::verify_data_column_sidecar_kzg_proofs;
use tracing::{error, info};
use tree_hash::TreeHash;

use crate::reconstruct::maybe_reconstruct;

/// Build a gossipsub config that only subscribes to the 128 column subnets.
pub fn da_gossipsub_config(fork_digest: B32) -> GossipsubConfig {
    let mut config = GossipsubConfig::default();

    let topics: Vec<GossipTopic> = (0..DATA_COLUMN_SIDECAR_SUBNET_COUNT)
        .map(|subnet_id| GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::DataColumnSidecar(subnet_id),
        })
        .collect();

    config.set_topics(topics);
    config
}

pub async fn handle_da_gossip_message(
    message: Message,
    store: &Arc<DaStore>,
    p2p_sender: &P2PSender,
    fork_digest: B32,
) {
    let sidecar = match GossipsubMessage::decode(&message.topic, &message.data) {
        Ok(GossipsubMessage::DataColumnSidecar(sidecar)) => sidecar,
        Ok(_) => return,
        Err(err) => {
            error!(topic = %message.topic, err = ?err, "Failed to decode gossip message");
            return;
        }
    };

    let subnet_id = match GossipTopic::from_topic_hash(&message.topic) {
        Ok(topic) => match topic.kind {
            GossipTopicKind::DataColumnSidecar(id) => id,
            _ => return,
        },
        Err(_) => return,
    };

    if !sidecar.verify() {
        error!(
            column = sidecar.index,
            "Column sidecar failed basic verification"
        );
        return;
    }

    if subnet_id != sidecar.compute_subnet() {
        error!(column = sidecar.index, "Column sidecar on wrong subnet");
        return;
    }

    if !sidecar.verify_inclusion_proof() {
        error!(
            column = sidecar.index,
            "Column sidecar inclusion proof invalid"
        );
        return;
    }

    match verify_data_column_sidecar_kzg_proofs(&sidecar) {
        Ok(true) => {}
        Ok(false) => {
            error!(column = sidecar.index, "Column sidecar KZG proofs invalid");
            return;
        }
        Err(err) => {
            error!(column = sidecar.index, err = ?err, "KZG verification error");
            return;
        }
    }

    let block_root = sidecar.signed_block_header.message.tree_hash_root();
    let column_index = sidecar.index;

    if let Err(err) = store.put_column(block_root, *sidecar).await {
        error!(column = column_index, err = ?err, "Failed to store column sidecar");
        return;
    }

    let count = store.column_count(block_root).await;

    // p2p_sender.send_gossip(GossipMessage {
    //     topic: GossipTopic::from_topic_hash(&message.topic).expect("valid topic"),
    //     data: sidecar_bytes,
    // });

    if count == 64 {
        info!(
            column = column_index,
            count,
            %block_root,
            "Reconstruction threshold reached, attempting reconstruction"
        );
        let store = store.clone();
        let p2p_sender = p2p_sender.clone();
        tokio::spawn(async move {
            maybe_reconstruct(block_root, &store, &p2p_sender, fork_digest).await;
        });
    } else if count == NUMBER_OF_COLUMNS as usize {
        info!(%block_root, "All 128 columns stored");
    }
}

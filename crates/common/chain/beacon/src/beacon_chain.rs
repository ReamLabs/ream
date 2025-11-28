use std::sync::Arc;

use anyhow::bail;
use ream_api_types_beacon::events::{BeaconEvent, HeadEvent};
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    electra::beacon_block::SignedBeaconBlock,
};
use ream_consensus_misc::constants::beacon::genesis_validators_root;
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice_beacon::{
    handlers::{on_attestation, on_attester_slashing, on_block, on_tick},
    store::Store,
};
use ream_network_spec::networks::beacon_network_spec;
use ream_operation_pool::OperationPool;
use ream_p2p::req_resp::beacon::messages::status::Status;
use ream_storage::{
    db::beacon::BeaconDB,
    tables::{field::REDBField, table::REDBTable},
};
use tokio::sync::{Mutex, broadcast};
use tracing::warn;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Mutex<Store>,
    pub execution_engine: Option<ExecutionEngine>,
    pub event_sender: Option<broadcast::Sender<BeaconEvent>>,
}

impl BeaconChain {
    /// Creates a new instance of `BeaconChain`.
    pub fn new(
        db: BeaconDB,
        operation_pool: Arc<OperationPool>,
        execution_engine: Option<ExecutionEngine>,
        event_sender: Option<broadcast::Sender<BeaconEvent>>,
    ) -> Self {
        Self {
            store: Mutex::new(Store::new(db, operation_pool)),
            execution_engine,
            event_sender,
        }
    }

    pub async fn process_block(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        let old_head = store.get_head().ok();

        on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
        )
        .await?;

        // Emit Block event
        if let Some(event_sender) = self.event_sender.as_ref() {
            tracing::debug!("Emitting Block event for slot {}", signed_block.message.slot);
            if let Err(e) = event_sender.send(BeaconEvent::Block(Box::new(signed_block.clone()))) {
                tracing::warn!("Failed to send Block event: {}", e);
            }
        }

        // Check for head change
        if let Ok(new_head) = store.get_head()
            && Some(new_head) != old_head
        {
            // Fetch block to get state root
            if let Ok(Some(block)) = store.db.block_provider().get(new_head) {
                let state_root = block.message.state_root;
                // For now, we use placeholders for dependent roots or try to calculate them if
                // possible. To do it properly we need the state.

                let head_event = HeadEvent {
                    slot: block.message.slot,
                    block: new_head,
                    state: state_root,
                    epoch_transition: false, // TODO: calculate
                    previous_duty_dependent_root: Default::default(), // TODO: calculate
                    current_duty_dependent_root: Default::default(), // TODO: calculate
                    execution_optimistic: false, // TODO
                };

                if let Some(event_sender) = self.event_sender.as_ref() {
                    let _ = event_sender.send(BeaconEvent::Head(head_event));
                }
            }
        }

        Ok(())
    }

    pub async fn process_attester_slashing(
        &self,
        attester_slashing: AttesterSlashing,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attester_slashing(&mut store, attester_slashing.clone())?;

        Ok(())
    }

    pub async fn process_attestation(
        &self,
        attestation: Attestation,
        is_from_block: bool,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attestation(&mut store, attestation.clone(), is_from_block)?;

        Ok(())
    }

    pub async fn process_tick(&self, time: u64) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_tick(&mut store, time)?;
        Ok(())
    }

    pub async fn build_status_request(&self) -> anyhow::Result<Status> {
        let Ok(finalized_checkpoint) = self
            .store
            .lock()
            .await
            .db
            .finalized_checkpoint_provider()
            .get()
        else {
            bail!("Failed to get finalized checkpoint");
        };

        let head_root = match self.store.lock().await.get_head() {
            Ok(head) => head,
            Err(err) => {
                warn!("Failed to get head root: {err}, falling back to finalized root");
                finalized_checkpoint.root
            }
        };

        let head_slot = match self.store.lock().await.db.block_provider().get(head_root) {
            Ok(Some(block)) => block.message.slot,
            err => {
                bail!("Failed to get block for head root {head_root}: {err:?}");
            }
        };

        Ok(Status {
            fork_digest: beacon_network_spec().fork_digest(genesis_validators_root()),
            finalized_root: finalized_checkpoint.root,
            finalized_epoch: finalized_checkpoint.epoch,
            head_root,
            head_slot,
        })
    }
}

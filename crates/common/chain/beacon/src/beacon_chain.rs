use std::sync::Arc;

use anyhow::bail;
use ream_consensus_beacon::{
    attestation::Attestation, attester_slashing::AttesterSlashing,
    electra::beacon_block::SignedBeaconBlock,
};
use ream_consensus_misc::constants::genesis_validators_root;
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice::{
    handlers::{on_attestation, on_attester_slashing, on_block, on_tick},
    store::Store,
};
use ream_network_spec::networks::beacon_network_spec;
use ream_operation_pool::OperationPool;
use ream_p2p::req_resp::messages::status::Status;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use tokio::sync::Mutex;
use tracing::warn;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Mutex<Store>,
    pub execution_engine: Option<ExecutionEngine>,
}

impl BeaconChain {
    /// Creates a new instance of `BeaconChain`.
    pub fn new(
        db: ReamDB,
        operation_pool: Arc<OperationPool>,
        execution_engine: Option<ExecutionEngine>,
    ) -> Self {
        Self {
            store: Mutex::new(Store::new(db, operation_pool)),
            execution_engine,
        }
    }

    pub async fn process_block(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_block(
            &mut store,
            &signed_block,
            &self.execution_engine,
            signed_block.message.slot >= beacon_network_spec().slot_n_days_ago(17),
        )
        .await?;
        Ok(())
    }

    pub async fn process_attester_slashing(
        &self,
        attester_slashing: AttesterSlashing,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attester_slashing(&mut store, attester_slashing)?;
        Ok(())
    }

    pub async fn process_attestation(
        &self,
        attestation: Attestation,
        is_from_block: bool,
    ) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_attestation(&mut store, attestation, is_from_block)?;
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

        let head_slot = match self
            .store
            .lock()
            .await
            .db
            .beacon_block_provider()
            .get(head_root)
        {
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

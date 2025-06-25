use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, bail, ensure};
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus::{
    attestation::Attestation,
    attester_slashing::AttesterSlashing,
    bls_to_execution_change::BLSToExecutionChange,
    constants::{MAX_BLOBS_PER_BLOCK_ELECTRA, genesis_validators_root},
    electra::beacon_block::SignedBeaconBlock,
    misc::compute_start_slot_at_epoch,
};
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice::{
    handlers::{on_attestation, on_attester_slashing, on_block, on_tick},
    store::Store,
};
use ream_network_spec::networks::network_spec;
use ream_operation_pool::OperationPool;
use ream_p2p::req_resp::messages::status::Status;
use ream_storage::{
    db::ReamDB,
    tables::{Field, Table},
};
use tokio::sync::{Mutex, RwLock};
use tracing::warn;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Mutex<Store>,
    pub execution_engine: Option<ExecutionEngine>,
    pub cached_proposer_signature: RwLock<HashMap<(PublicKey, u64), BLSSignature>>,
    pub cached_bls_to_execution_signature: RwLock<HashMap<(PublicKey, u64), BLSToExecutionChange>>,
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
            cached_proposer_signature: HashMap::new().into(),
            cached_bls_to_execution_signature: HashMap::new().into(),
        }
    }

    pub async fn process_block(&self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        on_block(&mut store, &signed_block, &self.execution_engine).await?;
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
            fork_digest: network_spec().fork_digest(genesis_validators_root()),
            finalized_root: finalized_checkpoint.root,
            finalized_epoch: finalized_checkpoint.epoch,
            head_root,
            head_slot,
        })
    }

    pub async fn validate_beacon_block(&self, block: &SignedBeaconBlock) -> anyhow::Result<()> {
        let store = self.store.lock().await;

        let latest_block_in_db = store.db.get_latest_block()?;
        let latest_state_in_db = store.db.get_latest_state()?;

        ensure!(
            block.message.slot > latest_block_in_db.message.slot,
            "Block slot must be greater than latest block slot in db"
        );

        let start_slot_at_epoch =
            compute_start_slot_at_epoch(store.db.finalized_checkpoint_provider().get()?.epoch);
        ensure!(
            block.message.slot >= start_slot_at_epoch,
            "Block slot must be greater than start slot at epoch"
        );

        let validator = latest_state_in_db
            .validators
            .get(block.message.proposer_index as usize)
            .ok_or(anyhow!("Invalid proposer index"))?;
        ensure!(
            !self
                .cached_proposer_signature
                .read()
                .await
                .contains_key(&(validator.public_key.clone(), block.message.slot)),
            format!(
                "Signature for slot:{} and proposer:{:?} already cached",
                block.message.slot, validator.public_key
            )
        );

        ensure!(
            latest_state_in_db.verify_block_signature(block)?,
            "Invalid block signature"
        );

        if let Some(parent) = store
            .db
            .beacon_block_provider()
            .get(block.message.parent_root)?
        {
            ensure!(
                block.message.slot > parent.message.slot + 1,
                "Invalid block slot"
            );
        } else {
            return Err(anyhow!("Invalid parent block"));
        }

        let finalized_checkpoint = store.db.finalized_checkpoint_provider().get()?;
        ensure!(
            store.get_checkpoint_block(block.message.parent_root, finalized_checkpoint.epoch)?
                == finalized_checkpoint.root,
            "Invalid finalized checkpoint"
        );

        ensure!(
            latest_state_in_db.get_beacon_proposer_index(Some(block.message.slot))?
                == block.message.proposer_index,
            "Invalid proposer index"
        );

        ensure!(
            block.message.body.execution_payload.timestamp
                == latest_state_in_db.compute_timestamp_at_slot(block.message.slot),
            "timestamp must be equal to expected timestamp at slot"
        );

        let proposer_bls_execution_change = &block
            .message
            .body
            .bls_to_execution_changes
            .get(block.message.proposer_index as usize)
            .ok_or(anyhow!("Invalid index for signed bls to execution change"))?
            .message;

        ensure!(
            !self
                .cached_bls_to_execution_signature
                .read()
                .await
                .contains_key(&(validator.public_key.clone(), block.message.proposer_index)),
            "BLS to execution signature already exists"
        );

        ensure!(
            block.message.body.blob_kzg_commitments.len() <= MAX_BLOBS_PER_BLOCK_ELECTRA as usize,
            "Too many blobs in block"
        );

        self.cached_proposer_signature.blocking_write().insert(
            (validator.public_key.clone(), block.message.slot),
            block.signature.clone(),
        );

        self.cached_bls_to_execution_signature
            .blocking_write()
            .insert(
                (validator.public_key.clone(), block.message.proposer_index),
                proposer_bls_execution_change.clone(),
            );

        Ok(())
    }
}

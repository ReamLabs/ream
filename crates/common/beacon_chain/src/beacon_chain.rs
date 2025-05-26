use ream_consensus::electra::beacon_block::SignedBeaconBlock;
use ream_execution_engine::ExecutionEngine;
use ream_fork_choice::{handlers::on_block, store::Store};
use ream_storage::db::ReamDB;

/// BeaconChain is the main struct which manages the nodes local beacon chain.
pub struct BeaconChain {
    pub store: Store,
    pub execution_engine: Option<ExecutionEngine>,
}

impl BeaconChain {
    /// Creates a new instance of `BeaconChain`.
    pub fn new(db: ReamDB, execution_engine: Option<ExecutionEngine>) -> Self {
        Self {
            store: Store::new(db),
            execution_engine,
        }
    }

    pub async fn process_block(&mut self, signed_block: SignedBeaconBlock) -> anyhow::Result<()> {
        on_block(&mut self.store, &signed_block, &self.execution_engine).await?;
        Ok(())
    }
}

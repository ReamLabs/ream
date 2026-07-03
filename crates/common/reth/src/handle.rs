#![warn(unused_imports)]

use std::{path::PathBuf, sync::Arc};

use alloy_genesis::Genesis;
use alloy_rpc_types_engine::{ExecutionData, ForkchoiceState, ForkchoiceUpdated, PayloadStatus};
use reth_ethereum::{
    chainspec::ChainSpec,
    engine::EthPayloadAttributes,
    node::{
        EthereumNode,
        builder::{NodeBuilder, NodeHandleFor},
        core::{
            args::{DatadirArgs, RpcServerArgs},
            node_config::NodeConfig,
        },
    },
    provider::db::{
        ClientVersion, Database, DatabaseEnv, database_metrics::DatabaseMetrics, init_db,
        mdbx::DatabaseArguments, test_utils::TempDatabase,
    },
    tasks::{RuntimeBuilder, RuntimeConfig, TokioConfig},
};
use reth_payload_builder::PayloadId;
use tokio::runtime::Handle;

use crate::{fork_choice, payload};

pub struct RethHandle<DB>
where
    DB: Database + DatabaseMetrics + Clone + Unpin + 'static,
{
    pub reth: NodeHandleFor<EthereumNode, DB>,
}

impl RethHandle<DatabaseEnv> {
    // Start a reth node with the given tokio runtime handle and persistent data directory.
    pub async fn start(ream_rt: Option<Handle>, datadir: PathBuf) -> eyre::Result<Self> {
        let mut config = RuntimeConfig::default();
        if let Some(handle) = ream_rt {
            config = config.with_tokio(TokioConfig::existing_handle(handle));
        }

        let reth_rt = RuntimeBuilder::new(config).build()?;

        let node_config = NodeConfig::new(custom_chain())
            .with_rpc(RpcServerArgs::default().with_http())
            .with_datadir_args(DatadirArgs {
                datadir: datadir.into(),
                ..Default::default()
            });

        let database = init_db(
            node_config.datadir().db(),
            DatabaseArguments::new(ClientVersion::default()),
        )?;

        let reth = NodeBuilder::new(node_config)
            .with_database(database)
            .with_launch_context(reth_rt)
            .node(EthereumNode::default())
            .launch_with_debug_capabilities()
            .await?;

        Ok(RethHandle { reth })
    }
}

impl RethHandle<Arc<TempDatabase<DatabaseEnv>>> {
    // Start a reth node with a temporary in-memory database for testing.
    pub async fn test() -> eyre::Result<Self> {
        let reth_rt = RuntimeBuilder::new(RuntimeConfig::default()).build()?;

        let reth = NodeBuilder::new(
            NodeConfig::new(custom_chain()).with_rpc(RpcServerArgs::default().with_http()),
        )
        .testing_node(reth_rt)
        .node(EthereumNode::default())
        .launch_with_debug_capabilities()
        .await?;

        Ok(RethHandle { reth })
    }
}

impl<DB> RethHandle<DB>
where
    DB: Database + DatabaseMetrics + Clone + Unpin + 'static,
{
    /// Sends a fork choice update to the execution layer.
    ///
    /// Called with `payload_attributes` to start building a payload for a
    /// proposal (the returned `payload_id` is fed to [`Self::build_payload`]),
    /// and with `None` to canonicalize the head after a block is imported.
    pub async fn update_forkchoice(
        &self,
        state: ForkchoiceState,
        payload_attributes: Option<EthPayloadAttributes>,
    ) -> eyre::Result<ForkchoiceUpdated> {
        fork_choice::update(
            self.reth.node.consensus_engine_handle(),
            state,
            payload_attributes,
        )
        .await
    }

    pub async fn build_payload(&self, payload_id: PayloadId) -> eyre::Result<ExecutionData> {
        payload::build(&self.reth.node.payload_builder_handle, payload_id).await
    }

    pub async fn import_payload(
        &self,
        execution_data: ExecutionData,
    ) -> eyre::Result<PayloadStatus> {
        payload::import(self.reth.node.consensus_engine_handle(), execution_data).await
    }

    /// For tests, the immediate runtime shutdown causes tasks to panic
    /// first fires the shutdown signal so all tasks exit their loops cleanly, then the runtime
    /// drops with no tasks in flight.
    pub async fn shutdown(self) {
        let executor = self.reth.node.task_executor.clone();
        tokio::task::spawn_blocking(move || {
            executor.graceful_shutdown_with_timeout(std::time::Duration::from_secs(5));
        })
        .await
        .ok();
    }
}

pub fn custom_chain() -> Arc<ChainSpec> {
    let custom_genesis = r#"
{
    "nonce": "0x42",
    "timestamp": "0x0",
    "extraData": "0x5343",
    "gasLimit": "0x1C9C380",
    "difficulty": "0x400000000",
    "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "coinbase": "0x0000000000000000000000000000000000000000",
    "alloc": {
        "0x6Be02d1d3665660d22FF9624b7BE0551ee1Ac91b": {
            "balance": "0x4a47e3c12448f4ad000000"
        }
    },
    "number": "0x0",
    "gasUsed": "0x0",
    "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "config": {
        "ethash": {},
        "chainId": 2600,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true,
        "shanghaiTime": 0,
        "cancunTime": 0,
        "pragueTime": 0,
        "amsterdamTime": 0
    }
}
"#;
    let genesis: Genesis = serde_json::from_str(custom_genesis).expect("genesis failed");
    Arc::new(genesis.into())
}

#[cfg(test)]
mod test {
    use alloy_primitives::{B256, hex};
    use alloy_rpc_types_engine::ForkchoiceState;
    use serial_test::serial;

    use super::*;
    use crate::fork_choice::{create_fork_choice_state, create_ream_payload_attributes};

    /// This test demonstrates a full EL payload execution/validation for a proposer
    #[tokio::test]
    #[serial]
    async fn test_proposer_el_payload() {
        let handle = RethHandle::test().await.unwrap();
        let genesis_hash = custom_chain().genesis_hash();
        let fork_choice_state: ForkchoiceState =
            create_fork_choice_state(genesis_hash, B256::ZERO, B256::ZERO);
        let payload_attrs = create_ream_payload_attributes(1, B256::ZERO, 0, 4);

        let fork_choice_updated = handle
            .update_forkchoice(fork_choice_state, Some(payload_attrs))
            .await
            .unwrap();

        let built_payload = handle
            .build_payload(fork_choice_updated.payload_id.unwrap())
            .await
            .unwrap();
        let payload_status = handle.import_payload(built_payload).await.unwrap();
        println!("{payload_status:?}");

        assert!(payload_status.is_valid());

        let new_block_hash = payload_status.latest_valid_hash.unwrap();
        // The second fcu call updates the head
        let second_fcu = handle
            .update_forkchoice(
                create_fork_choice_state(new_block_hash, new_block_hash, new_block_hash),
                None, //  just update head
            )
            .await
            .unwrap();

        assert!(second_fcu.payload_status.is_valid());
        handle.shutdown().await;
    }

    #[tokio::test]
    #[serial]
    async fn test_transaction_received() -> eyre::Result<()> {
        let node = RethHandle::test().await.unwrap();
        let _raw_tx = hex!(
            "02f876820a28808477359400847735940082520894ab0840c0e43688012c1adb0f5e3fc665188f83d28a029d394a5d630544000080c080a0a044076b7e67b5deecc63f61a8d7913fab86ca365b344b5759d1fe3563b4c39ea019eab979dd000da04dfc72bb0377c092d30fd9e1cab5ae487de49586cc8b0090"
        );
        // Only for test here, no need in actual node
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let response = client
        .post("http://127.0.0.1:8545")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_sendRawTransaction",
            "params": [
                "0x02f876820a28808477359400847735940082520894ab0840c0e43688012c1adb0f5e3fc665188f83d28a029d394a5d630544000080c080a0a044076b7e67b5deecc63f61a8d7913fab86ca365b344b5759d1fe3563b4c39ea019eab979dd000da04dfc72bb0377c092d30fd9e1cab5ae487de49586cc8b0090"
            ],
            "id": 1
        }))
        .send()
        .await?;

        println!("{}", response.text().await?);

        node.shutdown().await;
        Ok(())
    }
}

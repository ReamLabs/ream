#![warn(unused_imports)]

use std::{path::PathBuf, sync::Arc};

use alloy_genesis::Genesis;
use alloy_primitives::B256;
use alloy_rpc_types_engine::{ExecutionData, ForkchoiceState, ForkchoiceUpdated, PayloadStatus};
use reth_ethereum::{
    chainspec::ChainSpec,
    engine::EthPayloadAttributes,
    node::{
        EthEngineTypes, EthereumNode,
        api::ConsensusEngineHandle,
        builder::{NodeBuilder, NodeHandleFor},
        core::{
            args::{DatadirArgs, RpcServerArgs},
            node_config::NodeConfig,
        },
    },
    provider::db::{ClientVersion, DatabaseEnv, init_db, mdbx::DatabaseArguments},
    tasks::{RuntimeBuilder, RuntimeConfig, TokioConfig},
};
use reth_payload_builder::{PayloadBuilderHandle, PayloadId};
use tokio::runtime::Handle;

use crate::{fork_choice, payload};

pub type RethNode = NodeHandleFor<EthereumNode, DatabaseEnv>;

/// Cheaply-cloneable, `Send + Sync`
#[derive(Clone)]
pub struct RethHandle {
    payload_builder: PayloadBuilderHandle<EthEngineTypes>,
    engine: ConsensusEngineHandle<EthEngineTypes>,
    /// EL genesis block hash, captured at boot. The CL uses this to resolve a "build on genesis"
    /// request (the lean genesis carries a zero EL payload hash) to the EL's real genesis head.
    genesis_hash: B256,
}

impl RethHandle {
    // Start a reth node with the given tokio runtime handle.
    pub async fn start(
        ream_rt: Option<Handle>,
        datadir: PathBuf,
    ) -> eyre::Result<(Self, RethNode)> {
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

        let node = NodeBuilder::new(node_config)
            .with_database(database)
            .with_launch_context(reth_rt)
            .node(EthereumNode::default())
            .launch_with_debug_capabilities()
            .await?;

        let handle = RethHandle {
            payload_builder: node.node.payload_builder_handle.clone(),
            engine: node.node.consensus_engine_handle().clone(),
            genesis_hash: node.node.chain_spec().genesis_hash(),
        };

        Ok((handle, node))
    }

    /// EL genesis block hash. Callers resolve a zero parent hash (the lean genesis placeholder)
    /// to this before asking the EL to build on genesis.
    pub fn genesis_hash(&self) -> B256 {
        self.genesis_hash
    }

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
        fork_choice::update(&self.engine, state, payload_attributes).await
    }

    pub async fn build_payload(&self, payload_id: PayloadId) -> eyre::Result<ExecutionData> {
        payload::build(&self.payload_builder, payload_id).await
    }

    pub async fn import_payload(
        &self,
        execution_data: ExecutionData,
    ) -> eyre::Result<PayloadStatus> {
        payload::import(&self.engine, execution_data).await
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

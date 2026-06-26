#![warn(unused)]

use alloy_genesis::Genesis;
use alloy_primitives::{B256, b256, hex};
use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        EthEngineTypes, EthereumNode,
        builder::{NodeBuilder, NodeHandle},
        core::node_config::NodeConfig,
    },
    tasks::Runtime,
};
use std::sync::Arc;

use reth_ethereum::node::api::{ConsensusEngineHandle, PayloadTypes};
use reth_payload_builder::PayloadBuilderHandle;

#[derive(Debug)]
pub struct ReamRethHandle<T: PayloadTypes> {
    pub consensus_engine_handle: ConsensusEngineHandle<T>,
    pub payload_builder_handle: PayloadBuilderHandle<T>,
}

impl<T: PayloadTypes> ReamRethHandle<T> {
    pub fn new(
        consensus_engine_handle: ConsensusEngineHandle<T>,
        payload_builder_handle: PayloadBuilderHandle<T>,
    ) -> Self {
        Self {
            consensus_engine_handle,
            payload_builder_handle,
        }
    }
}

pub async fn start_reth() -> eyre::Result<ReamRethHandle<EthEngineTypes>> {
    let runtime = Runtime::test();

    let node_config = NodeConfig::test()
        .dev()
        .with_chain(custom_chain())
        .with_unused_ports();

    let NodeHandle {
        node,
        node_exit_future,
    } = NodeBuilder::new(node_config)
        .testing_node(runtime)
        .node(EthereumNode::default())
        .launch_with_debug_capabilities()
        .await?;

    let reth_handles = ReamRethHandle::new(
        node.consensus_engine_handle().clone(),
        node.payload_builder_handle.clone(),
    );

    tokio::spawn(async move {
        let _node = node;
        let _ = node_exit_future.await;
    });

    Ok(reth_handles)
}

fn custom_chain() -> Arc<ChainSpec> {
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
    let genesis: Genesis = serde_json::from_str(custom_genesis).unwrap();
    Arc::new(genesis.into())
}

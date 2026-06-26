#![warn(unused_imports)]

use std::sync::Arc;

use alloy_genesis::Genesis;
use reth_ethereum::{
    chainspec::ChainSpec,
    node::{
        EthEngineTypes, EthereumNode,
        api::{ConsensusEngineHandle, PayloadTypes},
        builder::{NodeBuilder, NodeHandle},
        core::{args::RpcServerArgs, node_config::NodeConfig},
    },
    tasks::Runtime,
};
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
        .with_rpc(RpcServerArgs::default().with_http())
        .with_chain(custom_chain());

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
    use alloy_rpc_types_engine::{ForkchoiceState, PayloadStatusEnum};
    use serial_test::serial;

    use super::*;
    use crate::fork_choice::{create_fork_choice_state, create_ream_payload_attributes};

    #[tokio::test]
    #[serial]
    async fn test_fork_choice_update() {
        let handle = start_reth().await.unwrap();
        let genesis_hash = custom_chain().genesis_hash();
        let fork_choice_state: ForkchoiceState =
            create_fork_choice_state(genesis_hash, B256::ZERO, B256::ZERO);
        let payload_attrs = create_ream_payload_attributes(1, B256::ZERO, 0, 4);

        let fork_choice_updated = handle
            .consensus_engine_handle
            .fork_choice_updated(fork_choice_state, Some(payload_attrs))
            .await
            .unwrap();

        assert_eq!(
            fork_choice_updated.payload_status.status,
            PayloadStatusEnum::Valid
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_transaction_received() -> eyre::Result<()> {
        let _node = start_reth().await.unwrap();
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

        Ok(())
    }
}

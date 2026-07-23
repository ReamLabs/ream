use std::collections::HashMap;

use alloy_primitives::{Address, B64, B256, U256};
use anyhow::{anyhow, bail};
use ream_execution_rpc_types::{
    electra::execution_payload::ExecutionPayload,
    execution_payload::ExecutionPayloadV3,
    forkchoice_update::{ForkchoiceStateV1, PayloadAttributesV3},
    get_payload::{BlobsBundleV1, PayloadV4},
    payload_status::{PayloadStatus, PayloadStatusV1},
};
use ssz_types::VariableList;

#[derive(Debug)]
pub struct ForkchoiceUpdated {
    pub payload_status: PayloadStatusV1,
    pub payload_id: Option<B64>,
}

#[derive(Debug)]
pub struct ExecutionBlockGenerator {
    blocks: HashMap<B256, ExecutionPayload>,
    pending_payloads: HashMap<B64, ExecutionPayload>,
    next_payload_id: u64,
    head_block_hash: B256,
}

impl ExecutionBlockGenerator {
    pub fn new(genesis_block_hash: B256) -> Self {
        let mut genesis_payload = ExecutionPayload {
            block_hash: genesis_block_hash,
            ..Default::default()
        };
        if genesis_block_hash == B256::ZERO {
            genesis_payload.block_hash = genesis_payload
                .to_execution_header(B256::ZERO, &[])
                .hash_slow();
        }

        let mut blocks = HashMap::new();
        blocks.insert(genesis_payload.block_hash, genesis_payload);

        Self {
            blocks,
            pending_payloads: HashMap::new(),
            next_payload_id: 1,
            head_block_hash: genesis_block_hash,
        }
    }

    pub fn forkchoice_updated(
        &mut self,
        state: ForkchoiceStateV1,
        attrs: Option<PayloadAttributesV3>,
    ) -> anyhow::Result<ForkchoiceUpdated> {
        self.head_block_hash = state.head_block_hash;

        let payload_id = attrs
            .map(|attrs| {
                let payload = self.build_new_execution_payload(state.head_block_hash, attrs)?;
                let payload_id = self.next_payload_id();
                self.pending_payloads.insert(payload_id, payload);
                Ok::<_, anyhow::Error>(payload_id)
            })
            .transpose()?;

        Ok(ForkchoiceUpdated {
            payload_status: self.valid_payload_status(Some(state.head_block_hash)),
            payload_id,
        })
    }

    pub fn build_new_execution_payload(
        &mut self,
        parent_hash: B256,
        attrs: PayloadAttributesV3,
    ) -> anyhow::Result<ExecutionPayload> {
        let block_number = self
            .blocks
            .get(&parent_hash)
            .map(|parent| parent.block_number + 1)
            .unwrap_or_default();
        let mut payload = ExecutionPayload {
            parent_hash,
            fee_recipient: attrs.suggested_fee_recipient,
            prev_randao: attrs.prev_randao,
            timestamp: attrs.timestamp,
            withdrawals: attrs.withdrawals,
            block_number,
            gas_limit: 30_000_000,
            base_fee_per_gas: U256::from(1),
            ..Default::default()
        };
        payload.block_hash = payload
            .to_execution_header(attrs.parent_beacon_block_root, &[])
            .hash_slow();
        self.blocks.insert(payload.block_hash, payload.clone());
        Ok(payload)
    }

    pub fn get_payload(&mut self, id: &B64) -> anyhow::Result<PayloadV4> {
        let payload = self
            .pending_payloads
            .remove(id)
            .ok_or_else(|| anyhow!("unknown payload id {id}"))?;

        Ok(PayloadV4 {
            execution_payload: ExecutionPayloadV3::from(payload),
            block_value: B256::with_last_byte(1),
            blobs_bundle: BlobsBundleV1 {
                blobs: VariableList::empty(),
                commitments: VariableList::empty(),
                proofs: VariableList::empty(),
            },
            should_override_builder: false,
            execution_requests: Vec::new(),
        })
    }

    pub fn new_payload(&mut self, payload: ExecutionPayloadV3) -> PayloadStatusV1 {
        let payload = payload_v3_to_execution_payload(payload);
        self.head_block_hash = payload.block_hash;
        self.blocks
            .entry(payload.block_hash)
            .or_insert(payload.clone());
        self.valid_payload_status(Some(payload.block_hash))
    }

    fn next_payload_id(&mut self) -> B64 {
        let payload_id = payload_id_from_counter(self.next_payload_id);
        self.next_payload_id += 1;
        payload_id
    }

    fn valid_payload_status(&self, latest_valid_hash: Option<B256>) -> PayloadStatusV1 {
        PayloadStatusV1 {
            status: PayloadStatus::Valid,
            latest_valid_hash,
            validation_error: None,
        }
    }
}

pub fn payload_id_from_counter(counter: u64) -> B64 {
    B64::from(counter.to_be_bytes())
}

pub fn genesis_execution_payload(parent_hash: B256, timestamp: u64) -> ExecutionPayload {
    let mut payload = ExecutionPayload {
        parent_hash,
        timestamp,
        gas_limit: 30_000_000,
        base_fee_per_gas: U256::from(1),
        ..Default::default()
    };
    payload.block_hash = payload.to_execution_header(B256::ZERO, &[]).hash_slow();
    payload
}

pub fn payload_v3_to_execution_payload(payload: ExecutionPayloadV3) -> ExecutionPayload {
    ExecutionPayload {
        parent_hash: payload.parent_hash,
        fee_recipient: payload.fee_recipient,
        state_root: payload.state_root,
        receipts_root: payload.receipts_root,
        logs_bloom: payload.logs_bloom,
        prev_randao: payload.prev_randao,
        block_number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data,
        base_fee_per_gas: payload.base_fee_per_gas,
        block_hash: payload.block_hash,
        transactions: payload.transactions,
        withdrawals: payload.withdrawals,
        blob_gas_used: payload.blob_gas_used,
        excess_blob_gas: payload.excess_blob_gas,
    }
}

pub fn ensure_valid_status(status: &PayloadStatusV1) -> anyhow::Result<()> {
    if status.status == PayloadStatus::Valid {
        Ok(())
    } else {
        bail!("unexpected payload status: {:?}", status.status)
    }
}

pub fn test_payload_attributes(parent_beacon_block_root: B256) -> PayloadAttributesV3 {
    PayloadAttributesV3 {
        timestamp: 1,
        prev_randao: B256::with_last_byte(2),
        suggested_fee_recipient: Address::with_last_byte(3),
        withdrawals: VariableList::empty(),
        parent_beacon_block_root,
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use ream_execution_rpc_types::forkchoice_update::ForkchoiceStateV1;

    use super::*;

    fn forkchoice_state(head_block_hash: B256) -> ForkchoiceStateV1 {
        ForkchoiceStateV1 {
            head_block_hash,
            safe_block_hash: head_block_hash,
            finalized_block_hash: head_block_hash,
        }
    }

    #[test]
    fn block_hash_is_real() {
        let genesis_hash = B256::with_last_byte(1);
        let parent_beacon_block_root = B256::with_last_byte(2);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);
        let payload = generator
            .build_new_execution_payload(
                genesis_hash,
                test_payload_attributes(parent_beacon_block_root),
            )
            .expect("payload builds");

        assert_eq!(
            payload.block_hash,
            payload
                .to_execution_header(parent_beacon_block_root, &[])
                .hash_slow()
        );
    }

    #[test]
    fn fcu_with_attrs_then_get_payload_round_trips() {
        let genesis_hash = B256::with_last_byte(1);
        let parent_beacon_block_root = B256::with_last_byte(4);
        let attrs = test_payload_attributes(parent_beacon_block_root);
        let expected_timestamp = attrs.timestamp;
        let expected_prev_randao = attrs.prev_randao;
        let expected_fee_recipient = attrs.suggested_fee_recipient;
        let expected_withdrawals = attrs.withdrawals.clone();
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);

        let response = generator
            .forkchoice_updated(forkchoice_state(genesis_hash), Some(attrs))
            .expect("forkchoice update succeeds");
        let payload = generator
            .get_payload(&response.payload_id.expect("payload id"))
            .expect("payload exists")
            .execution_payload;

        assert_eq!(payload.parent_hash, genesis_hash);
        assert_eq!(payload.timestamp, expected_timestamp);
        assert_eq!(payload.prev_randao, expected_prev_randao);
        assert_eq!(payload.fee_recipient, expected_fee_recipient);
        assert_eq!(payload.withdrawals, expected_withdrawals);
    }

    #[test]
    fn fcu_without_attrs_has_no_payload_id() {
        let genesis_hash = B256::with_last_byte(1);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);

        let response = generator
            .forkchoice_updated(forkchoice_state(genesis_hash), None)
            .expect("forkchoice update succeeds");

        assert_eq!(response.payload_id, None);
    }

    #[test]
    fn payloads_chain_by_parent_hash() {
        let genesis_hash = B256::with_last_byte(1);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);
        let first = generator
            .forkchoice_updated(
                forkchoice_state(genesis_hash),
                Some(test_payload_attributes(B256::with_last_byte(2))),
            )
            .expect("first forkchoice succeeds");
        let first_payload = generator
            .get_payload(&first.payload_id.expect("first payload id"))
            .expect("first payload exists")
            .execution_payload;

        let second = generator
            .forkchoice_updated(
                forkchoice_state(first_payload.block_hash),
                Some(test_payload_attributes(B256::with_last_byte(3))),
            )
            .expect("second forkchoice succeeds");
        let second_payload = generator
            .get_payload(&second.payload_id.expect("second payload id"))
            .expect("second payload exists")
            .execution_payload;

        assert_eq!(second_payload.parent_hash, first_payload.block_hash);
    }

    #[test]
    fn get_payload_unknown_id_is_error() {
        let mut generator = ExecutionBlockGenerator::new(B256::with_last_byte(1));

        assert!(generator.get_payload(&B64::with_last_byte(99)).is_err());
    }
}

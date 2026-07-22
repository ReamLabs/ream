use std::{collections::HashMap, sync::OnceLock};

use alloy_primitives::{Address, B64, B256, Bytes, U64, U256};
use alloy_rlp::Encodable;
use anyhow::{anyhow, bail, ensure};
use ream_consensus_misc::polynomial_commitments::{
    kzg_commitment::KZGCommitment, kzg_proof::KZGProof,
};
use ream_execution_rpc_types::{
    electra::execution_payload::ExecutionPayload,
    execution_payload::ExecutionPayloadV3,
    forkchoice_update::{ForkchoiceStateV1, PayloadAttributesV3},
    get_blobs::{Blob, BlobAndProofV1},
    get_payload::{BlobsBundleV1, PayloadV4},
    payload_status::{PayloadStatus, PayloadStatusV1},
    transaction::{AccessList, BlobTransaction, ToAddress},
};
use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
use ssz_types::{FixedVector, VariableList};

const BYTES_PER_BLOB: usize = 131_072;
const BLOB_TRANSACTION_TYPE: u8 = 3;

fn encode_blob_transaction(blob_versioned_hashes: Vec<B256>) -> Vec<u8> {
    let transaction = BlobTransaction {
        chain_id: U256::ZERO,
        nonce: U256::ZERO,
        max_priority_fee_per_gas: U256::ZERO,
        max_fee_per_gas: U256::ZERO,
        gas_limit: U256::ZERO,
        to: ToAddress::Empty,
        value: U256::ZERO,
        data: Bytes::new(),
        access_list: AccessList::default(),
        max_fee_per_blob_gas: U256::ZERO,
        blob_versioned_hashes,
        y_parity: U64::ZERO,
        r: U256::ZERO,
        s: U256::ZERO,
    };
    let mut bytes = vec![BLOB_TRANSACTION_TYPE];
    transaction.encode(&mut bytes);
    bytes
}

/// Shared `DASContext`, built once (loading the trusted setup is expensive).
fn das_context() -> &'static DASContext {
    static DAS_CONTEXT: OnceLock<DASContext> = OnceLock::new();
    DAS_CONTEXT.get_or_init(|| DASContext::new(&TrustedSetup::default(), UsePrecomp::No))
}

fn sample_blob(seed: u8) -> anyhow::Result<Blob> {
    let mut bytes = vec![0u8; BYTES_PER_BLOB];
    for (chunk_index, chunk) in bytes.chunks_mut(32).enumerate() {
        chunk[31] = seed.wrapping_add(chunk_index as u8);
    }
    Ok(Blob {
        inner: FixedVector::new(bytes)
            .map_err(|err| anyhow!("failed to build sample blob: {err:?}"))?,
    })
}

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
    blobs_per_payload: usize,
    pending_blobs: HashMap<B256, Vec<(Blob, KZGCommitment)>>,
    blobs_by_versioned_hash: HashMap<B256, Blob>,
    next_blob_seed: u8,
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
            blobs_per_payload: 0,
            pending_blobs: HashMap::new(),
            blobs_by_versioned_hash: HashMap::new(),
            next_blob_seed: 0,
        }
    }

    pub fn set_blobs_per_payload(&mut self, count: usize) {
        self.blobs_per_payload = count;
    }

    pub fn get_blob_and_proof(&self, versioned_hash: B256) -> Option<BlobAndProofV1> {
        self.blobs_by_versioned_hash
            .get(&versioned_hash)
            .map(|blob| BlobAndProofV1 {
                blob: blob.clone(),
                proof: KZGProof::default(),
            })
    }

    fn generate_blobs(&mut self) -> anyhow::Result<Vec<(Blob, KZGCommitment)>> {
        if self.blobs_per_payload == 0 {
            return Ok(Vec::new());
        }

        let context = das_context();
        let mut blobs_and_commitments = Vec::with_capacity(self.blobs_per_payload);
        for _ in 0..self.blobs_per_payload {
            let blob = sample_blob(self.next_blob_seed)?;
            self.next_blob_seed = self.next_blob_seed.wrapping_add(1);

            let blob_data = blob.inner.to_vec();
            ensure!(
                blob_data.len() == BYTES_PER_BLOB,
                "sample blob has unexpected length: {}",
                blob_data.len()
            );
            let blob_bytes: [u8; BYTES_PER_BLOB] =
                blob_data.try_into().expect("length already checked above");
            let commitment = KZGCommitment(
                context
                    .blob_to_kzg_commitment(&blob_bytes)
                    .map_err(|err| anyhow!("failed to compute sample blob commitment: {err:?}"))?,
            );

            blobs_and_commitments.push((blob, commitment));
        }
        Ok(blobs_and_commitments)
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

        let blobs_and_commitments = self.generate_blobs()?;
        if !blobs_and_commitments.is_empty() {
            let versioned_hashes = blobs_and_commitments
                .iter()
                .map(|(_, commitment)| commitment.calculate_versioned_hash())
                .collect();
            let transaction = VariableList::new(encode_blob_transaction(versioned_hashes))
                .map_err(|err| anyhow!("blob transaction too large: {err:?}"))?;
            payload.transactions = VariableList::new(vec![transaction])
                .map_err(|err| anyhow!("too many transactions: {err:?}"))?;
        }

        // Must run after `transactions` is set - the header hash commits to their root.
        payload.block_hash = payload
            .to_execution_header(attrs.parent_beacon_block_root, &[])
            .hash_slow();
        self.blocks.insert(payload.block_hash, payload.clone());
        if !blobs_and_commitments.is_empty() {
            self.pending_blobs
                .insert(payload.block_hash, blobs_and_commitments);
        }
        Ok(payload)
    }

    pub fn get_payload(&mut self, id: &B64) -> anyhow::Result<PayloadV4> {
        let payload = self
            .pending_payloads
            .remove(id)
            .ok_or_else(|| anyhow!("unknown payload id {id}"))?;

        let blobs_and_commitments = self
            .pending_blobs
            .remove(&payload.block_hash)
            .unwrap_or_default();
        for (blob, commitment) in &blobs_and_commitments {
            self.blobs_by_versioned_hash
                .insert(commitment.calculate_versioned_hash(), blob.clone());
        }
        let (blobs, commitments): (Vec<_>, Vec<_>) = blobs_and_commitments.into_iter().unzip();
        let proofs = vec![KZGProof::default(); blobs.len()];
        let blobs_bundle = BlobsBundleV1 {
            blobs: VariableList::new(blobs)
                .map_err(|err| anyhow!("too many sample blobs: {err:?}"))?,
            commitments: VariableList::new(commitments)
                .map_err(|err| anyhow!("too many sample commitments: {err:?}"))?,
            proofs: VariableList::new(proofs)
                .map_err(|err| anyhow!("too many sample proofs: {err:?}"))?,
        };

        Ok(PayloadV4 {
            execution_payload: ExecutionPayloadV3::from(payload),
            block_value: B256::with_last_byte(1),
            blobs_bundle,
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

    #[test]
    fn get_payload_defaults_to_no_blobs() {
        let genesis_hash = B256::with_last_byte(1);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);

        let response = generator
            .forkchoice_updated(
                forkchoice_state(genesis_hash),
                Some(test_payload_attributes(B256::with_last_byte(2))),
            )
            .expect("forkchoice update succeeds");
        let payload = generator
            .get_payload(&response.payload_id.expect("payload id"))
            .expect("payload exists");

        assert!(payload.blobs_bundle.blobs.is_empty());
        assert!(payload.blobs_bundle.commitments.is_empty());
        assert!(payload.blobs_bundle.proofs.is_empty());
    }

    #[test]
    fn get_payload_with_blobs_produces_verifiable_commitments_and_retrievable_blobs() {
        let genesis_hash = B256::with_last_byte(1);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);
        generator.set_blobs_per_payload(2);

        let response = generator
            .forkchoice_updated(
                forkchoice_state(genesis_hash),
                Some(test_payload_attributes(B256::with_last_byte(2))),
            )
            .expect("forkchoice update succeeds");
        let payload = generator
            .get_payload(&response.payload_id.expect("payload id"))
            .expect("payload exists");

        assert_eq!(payload.blobs_bundle.blobs.len(), 2);
        assert_eq!(payload.blobs_bundle.commitments.len(), 2);
        assert_eq!(payload.blobs_bundle.proofs.len(), 2);

        let context = das_context();
        for (blob, commitment) in payload
            .blobs_bundle
            .blobs
            .iter()
            .zip(payload.blobs_bundle.commitments.iter())
        {
            let blob_bytes: [u8; BYTES_PER_BLOB] = blob.inner.to_vec().try_into().unwrap();
            let recomputed = context.blob_to_kzg_commitment(&blob_bytes).unwrap();
            assert_eq!(recomputed, commitment.0, "commitment must match the blob");

            let versioned_hash = commitment.calculate_versioned_hash();
            let blob_and_proof = generator
                .get_blob_and_proof(versioned_hash)
                .expect("blob should be retrievable by its versioned hash");
            assert_eq!(&blob_and_proof.blob, blob);
        }

        assert!(
            generator
                .get_blob_and_proof(B256::with_last_byte(0xff))
                .is_none()
        );
    }

    #[test]
    fn payload_with_blobs_passes_is_valid_versioned_hashes() {
        use ream_consensus_misc::execution_requests::ExecutionRequests;
        use ream_execution_engine::{
            is_valid_versioned_hashes, new_payload_request::NewPayloadRequest,
        };

        let genesis_hash = B256::with_last_byte(1);
        let parent_beacon_block_root = B256::with_last_byte(2);
        let mut generator = ExecutionBlockGenerator::new(genesis_hash);
        generator.set_blobs_per_payload(2);

        let response = generator
            .forkchoice_updated(
                forkchoice_state(genesis_hash),
                Some(test_payload_attributes(parent_beacon_block_root)),
            )
            .expect("forkchoice update succeeds");
        let payload = generator
            .get_payload(&response.payload_id.expect("payload id"))
            .expect("payload exists");

        let versioned_hashes: Vec<B256> = payload
            .blobs_bundle
            .commitments
            .iter()
            .map(|commitment| commitment.calculate_versioned_hash())
            .collect();

        // Same check `verify_and_notify_new_payload` runs before importing a block.
        let request = NewPayloadRequest {
            execution_payload: payload_v3_to_execution_payload(payload.execution_payload),
            versioned_hashes,
            parent_beacon_block_root,
            execution_requests: ExecutionRequests::default(),
        };
        assert!(
            is_valid_versioned_hashes(&request).expect("should not error"),
            "generated payload's transactions must reference the same versioned hashes as its blob_kzg_commitments"
        );
    }
}

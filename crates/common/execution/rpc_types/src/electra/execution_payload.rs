use alloy_consensus::{
    Header,
    proofs::{ordered_trie_root, ordered_trie_root_with_encoder},
};
use alloy_primitives::{Address, B64, B256, Bloom, Bytes, U256, b256};
use alloy_rlp::Encodable;
use ream_consensus_misc::{misc::checksummed_address, withdrawal::Withdrawal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    FixedVector, VariableList,
    serde_utils::{hex_fixed_vec, hex_var_list, list_of_hex_var_list},
    typenum::{self, U16, U32, U1048576, U1073741824},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::execution_payload_header::ExecutionPayloadHeader;

const EMPTY_UNCLE_ROOT_HASH: B256 =
    b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");

pub type Transactions = VariableList<VariableList<u8, U1073741824>, U1048576>;

#[derive(
    Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Default,
)]
pub struct ExecutionPayload {
    // Execution block header fields
    pub parent_hash: B256,
    #[serde(with = "checksummed_address")]
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    #[serde(with = "hex_fixed_vec")]
    pub logs_bloom: FixedVector<u8, typenum::U256>,
    pub prev_randao: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub block_number: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub gas_limit: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub gas_used: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub timestamp: u64,
    #[serde(with = "hex_var_list")]
    pub extra_data: VariableList<u8, U32>,
    #[serde(with = "serde_utils::quoted_u256")]
    pub base_fee_per_gas: U256,

    // Extra payload fields
    pub block_hash: B256,
    #[serde(with = "list_of_hex_var_list")]
    pub transactions: Transactions,
    pub withdrawals: VariableList<Withdrawal, U16>,
    #[serde(with = "serde_utils::quoted_u64")]
    pub blob_gas_used: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub excess_blob_gas: u64,
}

impl ExecutionPayload {
    pub fn to_execution_header(
        &self,
        parent_beacon_block_root: B256,
        execution_requests_list: &[Bytes],
    ) -> Header {
        let transactions = self
            .transactions
            .clone()
            .into_iter()
            .map(|transaction| Bytes::from(transaction.to_vec()))
            .collect::<Vec<_>>();
        let transactions_root = calculate_transactions_root(&transactions);
        let withdrawals_root = calculate_withdrawals_root(&self.withdrawals);
        Header {
            parent_hash: self.parent_hash,
            ommers_hash: EMPTY_UNCLE_ROOT_HASH,
            beneficiary: self.fee_recipient,
            state_root: self.state_root,
            transactions_root,
            receipts_root: self.receipts_root,
            logs_bloom: Bloom::from_slice(&self.logs_bloom),
            difficulty: U256::ZERO,
            number: self.block_number,
            gas_limit: self.gas_limit,
            gas_used: self.gas_used,
            timestamp: self.timestamp,
            extra_data: Bytes::from(Vec::from(self.extra_data.clone())),
            // EIP-4399 repurposed the header's `mixHash` slot to carry the beacon chain's
            // `prev_randao`. Leaving it zero makes the recomputed hash differ from every
            // real block, so `is_valid_block_hash` rejects the payload before the execution
            // engine is ever consulted.
            mix_hash: self.prev_randao,
            nonce: B64::ZERO,
            base_fee_per_gas: Some(self.base_fee_per_gas.to::<u64>()),
            withdrawals_root: Some(withdrawals_root),
            blob_gas_used: Some(self.blob_gas_used),
            excess_blob_gas: Some(self.excess_blob_gas),
            parent_beacon_block_root: Some(parent_beacon_block_root),
            requests_hash: Some(compute_requests_hash(execution_requests_list)),
            block_access_list_hash: None,
            slot_number: None,
        }
    }

    pub fn to_execution_payload_header(&self) -> ExecutionPayloadHeader {
        ExecutionPayloadHeader {
            parent_hash: self.parent_hash,
            fee_recipient: self.fee_recipient,
            state_root: self.state_root,
            receipts_root: self.receipts_root,
            logs_bloom: self.logs_bloom.clone(),
            prev_randao: self.prev_randao,
            block_number: self.block_number,
            gas_limit: self.gas_limit,
            gas_used: self.gas_used,
            timestamp: self.timestamp,
            extra_data: self.extra_data.clone(),
            base_fee_per_gas: self.base_fee_per_gas,
            block_hash: self.block_hash,
            transactions_root: self.transactions.tree_hash_root(),
            withdrawals_root: self.withdrawals.tree_hash_root(),
            blob_gas_used: self.blob_gas_used,
            excess_blob_gas: self.excess_blob_gas,
        }
    }
}

fn compute_requests_hash(block_requests: &[Bytes]) -> B256 {
    let mut hasher = Sha256::new();

    for request in block_requests {
        if request.len() > 1 {
            let mut inner_hasher = Sha256::new();
            inner_hasher.update(request);
            let inner_hash = inner_hasher.finalize();
            hasher.update(inner_hash);
        }
    }

    let final_hash = hasher.finalize();
    B256::from_slice(&final_hash)
}

/// Calculate the Merkle Patricia Trie root hash from a list of items
/// `(rlp(index), encoded(item))` pairs.
pub fn calculate_transactions_root<T>(transactions: &[T]) -> B256
where
    T: Encodable,
{
    ordered_trie_root_with_encoder(transactions, |tx: &T, buf| tx.encode(buf))
}

/// Calculates the root hash of the withdrawals.
pub fn calculate_withdrawals_root(withdrawals: &[Withdrawal]) -> B256 {
    ordered_trie_root(withdrawals)
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{address, bytes};

    use super::*;

    /// Block 1 of a Kurtosis devnet, as reth reported it over `eth_getBlockByNumber`.
    /// Empty block, so the transactions/withdrawals/receipts roots are all the empty-trie
    /// root and the requests list is empty.
    fn devnet_block_1() -> ExecutionPayload {
        ExecutionPayload {
            parent_hash: b256!("9eba23403adfc7186ba8b484cf33b63edb357144427661022b1b9dac2b5637ed"),
            fee_recipient: address!("8943545177806ED17B9F23F0a21ee5948eCaa776"),
            state_root: b256!("f49c705b3d0a5fac1da62f0d8dfb218df0d4299a48ccf96157c6c1e35837eb14"),
            receipts_root: b256!("56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"),
            logs_bloom: FixedVector::new(vec![0u8; 256]).expect("bloom is 256 bytes"),
            prev_randao: b256!("9eba23403adfc7186ba8b484cf33b63edb357144427661022b1b9dac2b5637ed"),
            block_number: 1,
            gas_limit: 0x392a220,
            gas_used: 0,
            timestamp: 0x6a770600,
            extra_data: VariableList::new(bytes!("726574682f76322e342e312f6c696e7578").to_vec())
                .expect("extra data fits"),
            base_fee_per_gas: U256::from(0x342770c0u64),
            block_hash: b256!("ea01eaa48a97487659f390c6f5f04278db0a8d88bf9c0f3b1774e5290891d793"),
            transactions: Transactions::empty(),
            withdrawals: VariableList::empty(),
            blob_gas_used: 0,
            excess_blob_gas: 0,
        }
    }

    /// The recomputed header hash gates every block import: `is_valid_block_hash` rejects
    /// the payload before the execution engine is consulted, so any field dropped while
    /// rebuilding the header silently stops the node from ever following a chain.
    #[test]
    fn test_to_execution_header_reproduces_real_block_hash() {
        let payload = devnet_block_1();
        let parent_beacon_block_root =
            b256!("1031426cacba357a47278cb9d70407305cb155abc6ac6bb25d023adb8af04767");

        assert_eq!(
            payload
                .to_execution_header(parent_beacon_block_root, &[])
                .hash_slow(),
            payload.block_hash,
        );
    }

    /// EIP-4399: the header's `mixHash` slot carries `prev_randao`.
    #[test]
    fn test_to_execution_header_carries_prev_randao_as_mix_hash() {
        let payload = devnet_block_1();

        assert_eq!(
            payload.to_execution_header(B256::ZERO, &[]).mix_hash,
            payload.prev_randao,
        );
    }
}

use alloy_consensus::proofs::{ordered_trie_root, ordered_trie_root_with_encoder};
use alloy_primitives::{b256, bytes, keccak256, Address, Bytes, B256, B64, U256};
use alloy_rlp::Encodable;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    serde_utils::{hex_fixed_vec, hex_var_list, list_of_hex_var_list},
    typenum, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use crate::withdrawal::Withdrawal;

const EMPTY_UNCLE_ROOT_HASH: B256 =
    b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ExecutionPayload {
    // Execution block header fields
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    #[serde(with = "hex_fixed_vec")]
    pub logs_bloom: FixedVector<u8, typenum::U256>,
    pub prev_randao: B256,
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    #[serde(with = "hex_var_list")]
    pub extra_data: VariableList<u8, typenum::U32>,
    #[serde(with = "serde_utils::quoted_u256")]
    pub base_fee_per_gas: U256,

    // Extra payload fields
    pub block_hash: B256,
    #[serde(with = "list_of_hex_var_list")]
    pub transactions: VariableList<VariableList<u8, typenum::U1073741824>, typenum::U1048576>,
    pub withdrawals: VariableList<Withdrawal, typenum::U16>,
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
}

impl ExecutionPayload {
    pub fn header_hash(&self, parent_beacon_block_root: B256) -> B256 {
        let mut buf = vec![];
        self.encode(&mut buf, parent_beacon_block_root);
        keccak256(buf)
    }

    fn encode(&self, out: &mut dyn bytes::BufMut, parent_beacon_block_root: B256) {
        let transactions = self
            .transactions
            .clone()
            .into_iter()
            .map(|transaction| Bytes::from(transaction.to_vec()))
            .collect::<Vec<_>>();
        let transactions_root = calculate_transactions_root(&transactions);
        let withdrawals_root = calculate_withdrawals_root(&self.withdrawals);
        alloy_rlp::Header {
            list: true,
            payload_length: self.rlp_payload_length(
                parent_beacon_block_root,
                transactions_root,
                withdrawals_root,
            ),
        }
        .encode(out);

        self.parent_hash.encode(out);
        EMPTY_UNCLE_ROOT_HASH.encode(out);
        self.fee_recipient.encode(out);
        self.state_root.encode(out);
        transactions_root.encode(out);
        self.receipts_root.encode(out);
        self.logs_bloom.encode(out);
        U256::ZERO.encode(out);
        self.block_number.encode(out);
        self.gas_limit.encode(out);
        self.gas_used.encode(out);
        self.timestamp.encode(out);
        self.extra_data.to_vec().as_slice().encode(out);
        self.prev_randao.encode(out);
        B64::ZERO.encode(out);
        self.base_fee_per_gas.encode(out);
        withdrawals_root.encode(out);
        self.blob_gas_used.encode(out);
        self.excess_blob_gas.encode(out);
        parent_beacon_block_root.encode(out);
    }

    fn rlp_payload_length(
        &self,
        parent_beacon_block_root: B256,
        transactions_root: B256,
        withdrawals_root: B256,
    ) -> usize {
        self.parent_hash.length()
            + EMPTY_UNCLE_ROOT_HASH.length() // ommers_hash
            + self.fee_recipient.length()
            + self.state_root.length()
            + transactions_root.length()
            + self.receipts_root.length()
            + self.logs_bloom.length()
            + U256::ZERO.length() // difficulty
            + self.block_number.length()
            + self.gas_limit.length()
            + self.gas_used.length()
            + self.timestamp.length()
            + self.extra_data.to_vec().as_slice().length()
            + self.prev_randao.length()
            + B64::ZERO.length() // nonce
            + self.base_fee_per_gas.length()
            + withdrawals_root.length()
            + self.blob_gas_used.length()
            + self.excess_blob_gas.length()
            + parent_beacon_block_root.length()
    }
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

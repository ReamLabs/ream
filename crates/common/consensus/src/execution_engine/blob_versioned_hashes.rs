use alloy_primitives::B256;
use alloy_rlp::Decodable;
use anyhow::anyhow;

use super::rpc_types::transaction::{BlobTransaction, TransactionType};
use crate::deneb::execution_payload::ExecutionPayload;

pub fn blob_versioned_hashes(execution_payload: &ExecutionPayload) -> anyhow::Result<Vec<B256>> {
    let mut blob_versioned_hashes = vec![];
    for transaction in execution_payload.transactions.iter() {
        if TransactionType::try_from(&transaction[..])
            .map_err(|err| anyhow!("Failed to detect transaction type: {err:?}"))?
            == TransactionType::BlobTransaction
        {
            let blob_transaction = BlobTransaction::decode(&mut &transaction[1..])?;
            blob_versioned_hashes.extend(blob_transaction.blob_versioned_hashes);
        }
    }

    Ok(blob_versioned_hashes)
}

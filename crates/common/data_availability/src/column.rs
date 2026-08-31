use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use crate::{error::ValidationError, id::ColumnId};

/// Consensus-derived context attached to a candidate column.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ColumnContext {
    /// Slot of the block the column belongs to; used for retention only.
    pub slot: u64,
}

/// A candidate column submitted for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateColumn {
    pub id: ColumnId,
    pub context: ColumnContext,
    pub payload: Vec<u8>,
}

/// A column that passed verification — the only type accepted by
/// `ColumnWriteStore`, and only constructed by `ColumnVerifier` implementations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedColumn {
    id: ColumnId,
    context: ColumnContext,
    payload: Vec<u8>,
}

impl VerifiedColumn {
    pub fn new_unchecked(id: ColumnId, context: ColumnContext, payload: Vec<u8>) -> Self {
        Self {
            id,
            context,
            payload,
        }
    }

    pub fn id(&self) -> ColumnId {
        self.id
    }

    pub fn context(&self) -> ColumnContext {
        self.context
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// A whole block's worth of candidate columns, submitted as one unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBlock {
    block_root: B256,
    context: ColumnContext,
    columns: Vec<(u64, Vec<u8>)>,
}

impl CandidateBlock {
    /// Validate the batch structure: rejects an empty batch, out-of-range
    /// column indices, and duplicate column indices.
    pub fn new(
        block_root: B256,
        context: ColumnContext,
        columns: Vec<(u64, Vec<u8>)>,
    ) -> Result<Self, ValidationError> {
        if columns.is_empty() {
            return Err(ValidationError::EmptyBatch);
        }

        let mut seen = 0u128;
        for (index, _) in &columns {
            // Range check first, delegated to the id type — the single owner
            // of that invariant.
            ColumnId::new(block_root, *index)?;
            let bit = 1u128 << *index;
            if seen & bit != 0 {
                return Err(ValidationError::DuplicateColumnIndex {
                    column_index: *index,
                });
            }
            seen |= bit;
        }

        Ok(Self {
            block_root,
            context,
            columns,
        })
    }

    pub fn block_root(&self) -> B256 {
        self.block_root
    }

    pub fn context(&self) -> ColumnContext {
        self.context
    }

    pub fn columns(&self) -> &[(u64, Vec<u8>)] {
        &self.columns
    }

    pub fn columns_len(&self) -> usize {
        self.columns.len()
    }

    /// Explode into per-column candidates, in submission order.
    pub fn into_columns(self) -> impl Iterator<Item = CandidateColumn> {
        let block_root = self.block_root;
        let context = self.context;
        self.columns.into_iter().map(move |(index, payload)| {
            let id = ColumnId::new(block_root, index)
                .expect("batch indices are validated at construction");
            CandidateColumn {
                id,
                context,
                payload,
            }
        })
    }

    /// Decompose into `(block_root, context, columns)`
    pub fn into_parts(self) -> (B256, ColumnContext, Vec<(u64, Vec<u8>)>) {
        (self.block_root, self.context, self.columns)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;

    use super::{CandidateBlock, ColumnContext};
    use crate::{error::ValidationError, id::NUMBER_OF_COLUMNS};

    fn payload(byte: u8) -> Vec<u8> {
        vec![byte]
    }

    #[test]
    fn candidate_block_rejects_an_empty_batch() {
        assert_eq!(
            CandidateBlock::new(B256::ZERO, ColumnContext::default(), vec![]),
            Err(ValidationError::EmptyBatch),
        );
    }

    #[test]
    fn candidate_block_rejects_an_out_of_range_index() {
        let columns = vec![(0, payload(0)), (NUMBER_OF_COLUMNS, payload(1))];
        assert!(matches!(
            CandidateBlock::new(B256::ZERO, ColumnContext::default(), columns),
            Err(ValidationError::InvalidColumnIndex { .. }),
        ));
    }

    #[test]
    fn candidate_block_rejects_a_duplicate_index() {
        let columns = vec![(3, payload(0)), (5, payload(1)), (3, payload(2))];
        assert_eq!(
            CandidateBlock::new(B256::ZERO, ColumnContext::default(), columns),
            Err(ValidationError::DuplicateColumnIndex { column_index: 3 }),
        );
    }

    #[test]
    fn candidate_block_accepts_a_full_block_and_explodes_in_order() {
        let block_root = B256::repeat_byte(7);
        let context = ColumnContext { slot: 42 };
        let columns: Vec<_> = (0..NUMBER_OF_COLUMNS)
            .map(|index| (index, payload(index as u8)))
            .collect();

        let block = CandidateBlock::new(block_root, context, columns).expect("a valid batch");
        assert_eq!(block.columns_len(), NUMBER_OF_COLUMNS as usize);
        assert_eq!(block.block_root(), block_root);
        assert_eq!(block.context(), context);

        // Explodes into per-column candidates carrying the shared root/context,
        // in submission order.
        for (expected_index, candidate) in block.into_columns().enumerate() {
            assert_eq!(candidate.id.index(), expected_index as u64);
            assert_eq!(candidate.id.block_root(), block_root);
            assert_eq!(candidate.context, context);
            assert_eq!(candidate.payload, &[expected_index as u8]);
        }
    }

    #[test]
    fn candidate_block_round_trips_through_parts() {
        let columns = vec![(1, payload(1)), (9, payload(9))];
        let block = CandidateBlock::new(B256::ZERO, ColumnContext { slot: 5 }, columns.clone())
            .expect("a valid batch");

        let (root, context, parts) = block.into_parts();
        assert_eq!(root, B256::ZERO);
        assert_eq!(context.slot, 5);
        assert_eq!(parts, columns);
    }
}

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use crate::error::ValidationError;

/// Number of data columns per block in the PeerDAS MVP.
///
/// The MVP custodies and serves the full column set.
///
/// This MUST stay equal to the spec's `NUMBER_OF_COLUMNS`
/// (<https://ethereum.github.io/consensus-specs/fulu/das-core/>).
pub const NUMBER_OF_COLUMNS: u64 = 128;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize, Default,
)]
pub struct DaColumnId {
    block_root: B256,
    index: u64,
}

impl DaColumnId {
    pub fn new(block_root: B256, index: u64) -> Result<Self, ValidationError> {
        if index >= NUMBER_OF_COLUMNS {
            return Err(ValidationError::InvalidColumnIndex {
                column_index: (index),
                number_of_columns: (NUMBER_OF_COLUMNS),
            });
        }

        Ok(Self { block_root, index })
    }

    pub fn block_root(&self) -> B256 {
        self.block_root
    }

    pub fn index(&self) -> u64 {
        self.index
    }
}

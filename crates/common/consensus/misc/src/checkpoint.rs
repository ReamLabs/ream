use std::str::FromStr;

use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(
    Debug,
    Eq,
    Hash,
    PartialEq,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    Encode,
    Decode,
    TreeHash,
    PartialOrd,
    Ord,
    Default,
)]
pub struct Checkpoint {
    #[serde(with = "serde_utils::quoted_u64")]
    pub epoch: u64,
    pub root: B256,
}

impl FromStr for Checkpoint {
    type Err = CheckpointParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (root_str, epoch_str) = s
            .split_once(':')
            .ok_or(CheckpointParseError::InvalidFormat)?;

        let root = root_str
            .strip_prefix("0x")
            .ok_or(CheckpointParseError::MissingHexPrefix)
            .and_then(|hex| B256::from_str(hex).map_err(|_| CheckpointParseError::InvalidHex))?;

        let epoch = epoch_str
            .parse::<u64>()
            .map_err(|_| CheckpointParseError::InvalidEpoch)?;

        Ok(Self { epoch, root })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CheckpointParseError {
    #[error("Expected format: 0x<block_root>:<epoch>")]
    InvalidFormat,
    #[error("Missing '0x' prefix on block_root")]
    MissingHexPrefix,
    #[error("Invalid hex block_root (expected 32 bytes)")]
    InvalidHex,
    #[error("Epoch must be a valid u64 integer")]
    InvalidEpoch,
}

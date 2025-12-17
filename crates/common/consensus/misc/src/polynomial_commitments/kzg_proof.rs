use alloy_primitives::FixedBytes;

use crate::constants::beacon::BYTES_PER_PROOF;

pub type KZGProof = FixedBytes<BYTES_PER_PROOF>;

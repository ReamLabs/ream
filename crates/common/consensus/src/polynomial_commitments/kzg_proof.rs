use alloy_primitives::FixedBytes;
use kzg::eip_4844::BYTES_PER_PROOF;

pub type KZGProof = FixedBytes<BYTES_PER_PROOF>;

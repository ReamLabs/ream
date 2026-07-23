pub mod lean_multisig;
#[cfg(not(feature = "optimized-leanvm"))]
pub mod leansig;
#[cfg(feature = "optimized-leanvm")]
#[path = "leanvm_sig/mod.rs"]
pub mod leansig;

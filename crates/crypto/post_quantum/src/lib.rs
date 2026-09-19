pub mod lean_multisig;
#[cfg(all(feature = "optimized-leanvm", feature = "optimized-leanvm-b"))]
compile_error!("features `optimized-leanvm` and `optimized-leanvm-b` are mutually exclusive");
#[cfg(not(any(feature = "optimized-leanvm", feature = "optimized-leanvm-b")))]
pub mod leansig;
#[cfg(all(feature = "optimized-leanvm", not(feature = "optimized-leanvm-b")))]
#[path = "leanvm_sig/mod.rs"]
pub mod leansig;
#[cfg(all(feature = "optimized-leanvm-b", not(feature = "optimized-leanvm")))]
#[path = "leanvm_b_sig/mod.rs"]
pub mod leansig;
#[cfg(feature = "shadow-integration")]
pub mod shadow;

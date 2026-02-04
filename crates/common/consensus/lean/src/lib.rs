#[cfg(all(feature = "devnet2", feature = "devnet3"))]
compile_error!(
    "Features 'devnet2' and 'devnet3' are mutually exclusive. Use --no-default-features --features devnet3 to build for devnet3."
);

#[cfg(not(any(feature = "devnet2", feature = "devnet3")))]
compile_error!("Either 'devnet2' or 'devnet3' feature must be enabled.");
pub mod attestation;
pub mod block;
pub mod checkpoint;
pub mod config;
pub mod slot;
pub mod state;
pub mod utils;
pub mod validator;

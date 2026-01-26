#[cfg(all(feature = "devnet1", feature = "devnet2"))]
compile_error!(
    "Features 'devnet1' and 'devnet2' are mutually exclusive. Use --no-default-features --features devnet2 to build for devnet2."
);

#[cfg(not(any(feature = "devnet1", feature = "devnet2")))]
compile_error!("Either 'devnet1' or 'devnet2' feature must be enabled.");

pub mod attestation;
pub mod block;
pub mod checkpoint;
pub mod config;
pub mod slot;
pub mod state;
pub mod utils;
pub mod validator;

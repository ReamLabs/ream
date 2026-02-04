#[cfg(all(feature = "devnet2", feature = "devnet3"))]
compile_error!(
    "Features 'devnet2' and 'devnet3' are mutually exclusive. Use --no-default-features --features devnet3 to build for devnet3."
);

#[cfg(not(any(feature = "devnet2", feature = "devnet3")))]
compile_error!("Either 'devnet2' or 'devnet3' feature must be enabled.");

pub mod bootnodes;
pub mod config;
pub mod constants;
pub mod gossipsub;
pub mod network;
pub mod utils;

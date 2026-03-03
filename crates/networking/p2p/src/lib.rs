#[cfg(all(feature = "devnet3", feature = "devnet4"))]
compile_error!("Features 'devnet3' and 'devnet4' are mutually exclusive.");

#[cfg(not(any(feature = "devnet3", feature = "devnet4")))]
compile_error!("Either 'devnet3' or 'devnet4' feature must be enabled.");

pub mod bootnodes;
pub mod config;
pub mod constants;
pub mod gossipsub;
pub mod network;
pub mod utils;

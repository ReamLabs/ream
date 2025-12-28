#[cfg(all(feature = "devnet1", feature = "devnet2"))]
compile_error!("Features 'devnet1' and 'devnet2' are mutually exclusive. Use --no-default-features --features devnet2 to build for devnet2.");

#[cfg(not(any(feature = "devnet1", feature = "devnet2")))]
compile_error!("Either 'devnet1' or 'devnet2' feature must be enabled.");

pub mod bootnodes;
pub mod config;
pub mod constants;
pub mod gossipsub;
pub mod network;
pub mod req_resp;
pub mod utils;

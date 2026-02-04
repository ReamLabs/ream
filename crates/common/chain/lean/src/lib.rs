#[cfg(all(feature = "devnet2", feature = "devnet3"))]
compile_error!(
    "Features 'devnet2' and 'devnet3' are mutually exclusive. Use --no-default-features --features devnet3 to build for devnet3."
);

#[cfg(not(any(feature = "devnet2", feature = "devnet3")))]
compile_error!("Either 'devnet2' or 'devnet3' feature must be enabled.");

pub mod clock;
pub mod messages;
pub mod p2p_request;
pub mod service;
pub mod slot;
pub mod sync;

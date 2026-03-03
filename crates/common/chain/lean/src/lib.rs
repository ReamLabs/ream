#[cfg(all(feature = "devnet3", feature = "devnet4"))]
compile_error!("Features 'devnet3' and 'devnet4' are mutually exclusive.");

#[cfg(not(any(feature = "devnet3", feature = "devnet4")))]
compile_error!("Either 'devnet3' or 'devnet4' feature must be enabled.");

pub mod clock;
pub mod messages;
pub mod p2p_request;
pub mod service;
pub mod slot;
pub mod sync;

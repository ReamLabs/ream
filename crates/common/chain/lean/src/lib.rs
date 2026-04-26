#[cfg(all(feature = "devnet4", feature = "devnet5"))]
compile_error!("Features 'devnet4' and 'devnet5' are mutually exclusive.");

#[cfg(not(any(feature = "devnet4", feature = "devnet5")))]
compile_error!("Either 'devnet4' or 'devnet5' feature must be enabled.");

pub mod clock;
pub mod messages;
pub mod p2p_request;
pub mod service;
pub mod slot;
pub mod sync;

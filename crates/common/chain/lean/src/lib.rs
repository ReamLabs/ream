#[cfg(not(feature = "devnet5"))]
compile_error!("The 'devnet5' feature must be enabled.");

pub mod clock;
pub mod messages;
pub mod p2p_request;
pub mod service;
pub mod slot;
pub mod sync;

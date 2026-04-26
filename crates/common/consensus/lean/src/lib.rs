#[cfg(all(feature = "devnet4", feature = "devnet5"))]
compile_error!("Features 'devnet4' and 'devnet5' are mutually exclusive.");

#[cfg(not(any(feature = "devnet4", feature = "devnet5")))]
compile_error!("Either 'devnet4' or 'devnet5' feature must be enabled.");

pub mod attestation;
pub mod block;
pub mod checkpoint;
pub mod config;
pub mod slot;
pub mod state;
pub mod utils;
pub mod validator;

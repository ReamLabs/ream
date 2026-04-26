#[cfg(not(feature = "devnet4"))]
compile_error!("The 'devnet4' feature must be enabled.");

pub mod attestation;
pub mod block;
pub mod checkpoint;
pub mod config;
pub mod slot;
pub mod state;
pub mod utils;
pub mod validator;

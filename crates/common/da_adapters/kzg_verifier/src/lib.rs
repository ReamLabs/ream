//! PeerDAS/KZG verifier adapter for the DA core.
//!
//! This is the scheme boundary made concrete: the only DA crate that depends on
//! both beacon types (`DataColumnSidecar`) and a concrete commitment scheme
//! (KZG over BLS12-381). Isolating those dependencies here lets `ream-da` stay
//! free of beacon and KZG code while still getting real verification, plugged
//! in through the [`ream_da::verifier::DaVerifier`] trait.

pub mod verifier;

pub use verifier::KzgVerifier;

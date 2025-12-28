#[cfg(all(feature = "devnet1", feature = "devnet2"))]
compile_error!("Features 'devnet1' and 'devnet2' are mutually exclusive. Use --no-default-features --features devnet2 to build for devnet2.");

#[cfg(not(any(feature = "devnet1", feature = "devnet2")))]
compile_error!("Either 'devnet1' or 'devnet2' feature must be enabled.");

pub mod attestation;
pub mod block;
pub mod checkpoint;
pub mod config;
pub mod state;
pub mod utils;
pub mod validator;

pub fn is_justifiable_slot(finalized_slot: u64, candidate_slot: u64) -> bool {
    assert!(
        candidate_slot >= finalized_slot,
        "Candidate slot ({candidate_slot}) must be more than or equal to finalized slot ({finalized_slot})"
    );

    let delta = candidate_slot - finalized_slot;

    delta <= 5
    || (delta as f64).sqrt().fract() == 0.0 // any x^2
    || (delta as f64 + 0.25).sqrt() % 1.0 == 0.5 // any x^2+x
}

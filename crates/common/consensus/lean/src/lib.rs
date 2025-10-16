pub mod block;
pub mod checkpoint;
pub mod config;
pub mod state;
pub mod validator;
pub mod vote;

/// We allow justification of slots either <= 5 or a perfect square or oblong after
/// the latest finalized slot. This gives us a backoff technique and ensures
/// finality keeps progressing even under high latency
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

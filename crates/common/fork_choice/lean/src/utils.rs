use anyhow::ensure;

pub fn is_justifiable_after(candidate_slot: u64, finalized_slot: u64) -> anyhow::Result<bool> {
    ensure!(
        candidate_slot >= finalized_slot,
        "Candidate slot must not be before finalized slot"
    );
    let delta = candidate_slot - finalized_slot;
    Ok(delta <= 5
        || delta.isqrt().pow(2) == delta
        || (4 * delta + 1).isqrt().pow(2) == 4 * delta + 1 && (4 * delta + 1).isqrt() % 2 == 1)
}

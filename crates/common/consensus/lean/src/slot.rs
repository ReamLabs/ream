use anyhow::ensure;

pub fn is_justifiable_after(candidate_slot: u64, finalized_slot: u64) -> anyhow::Result<bool> {
    ensure!(
        candidate_slot >= finalized_slot,
        "Candidate slot ({candidate_slot}) must be more than or equal to finalized slot ({finalized_slot})"
    );
    let delta = candidate_slot - finalized_slot;
    Ok(delta <= 5
        || delta.isqrt().pow(2) == delta
        || (4 * delta + 1).isqrt().pow(2) == 4 * delta + 1 && (4 * delta + 1).isqrt() % 2 == 1)
}

#[cfg(feature = "devnet2")]
pub fn justified_index_after(candidate_slot: u64, finalized_slot: u64) -> Option<u64> {
    if candidate_slot <= finalized_slot {
        return None;
    }

    Some(candidate_slot - finalized_slot - 1)
}

pub fn is_justifiable_after(candidate_slot: u64, finalized_slot: u64) -> bool {
    if candidate_slot < finalized_slot {
        return false;
    }

    let delta = candidate_slot - finalized_slot;
    let pronic_discriminant = 4u128 * u128::from(delta) + 1;
    let discriminant_root = pronic_discriminant.isqrt();
    delta <= 5
        || delta.isqrt().pow(2) == delta
        || discriminant_root.pow(2) == pronic_discriminant && discriminant_root % 2 == 1
}

pub fn justified_index_after(candidate_slot: u64, finalized_slot: u64) -> Option<u64> {
    if candidate_slot <= finalized_slot {
        return None;
    }

    Some(candidate_slot - finalized_slot - 1)
}

#[cfg(test)]
mod tests {
    use super::is_justifiable_after;

    #[test]
    fn test_slot_one_before_finalized_not_justifiable() {
        assert!(!is_justifiable_after(9, 10));
    }

    #[test]
    fn test_slot_far_before_finalized_not_justifiable() {
        assert!(!is_justifiable_after(90, 100));
    }
}

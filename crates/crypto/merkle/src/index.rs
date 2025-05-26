use std::collections::HashSet;

pub fn get_generalized_index_bit(index: u64, position: u64) -> bool {
    (index & (1 << position)) > 0
}

pub const fn generalized_index_sibling(generalized_index: u64) -> u64 {
    generalized_index ^ 1
}

pub fn generalized_index_child(generalized_index: u64, right_side: bool) -> u64 {
    generalized_index * 2 + right_side as u64
}

pub const fn generalized_index_parent(generalized_index: u64) -> u64 {
    generalized_index / 2
}

pub fn get_subtree_index(generalized_index: u64) -> u64 {
    generalized_index % (1 << (generalized_index as f64).log2().floor() as u64)
}

/// Return the generalized index of the leaf index with ``depth``.
pub fn generalized_index_from_leaf_index(leaf_index: u64, depth: u64) -> u64 {
    leaf_index + (1 << depth)
}

/// Get the generalized indices of the sister chunks along the
/// path from the chunk with the given tree index to the root.
fn get_branch_indices(generalized_index: u64) -> Vec<u64> {
    let mut focus = generalized_index_sibling(generalized_index);
    let mut result = vec![focus];
    while focus > 1 {
        focus = generalized_index_sibling(generalized_index_parent(focus));
        result.push(focus);
    }
    result.pop();
    result
}

/// Get the generalized indices of the chunks along
/// the path from the chunk with the given tree index to the root.
fn get_path_indices(generalized_index: u64) -> Vec<u64> {
    let mut focus = generalized_index;
    let mut result = vec![focus];
    while focus > 1 {
        focus = generalized_index_parent(focus);
        result.push(focus);
    }
    result.pop();
    result
}

/// Get the generalized indices of all "extra" chunks in the tree needed to
/// prove the chunks with the given generalized indices.
/// Note that the decreasing order is chosen deliberately
/// to ensure equivalence to the order of hashes in a regular
/// single-item Merkle proof in the single-item case.
pub fn get_helper_indices(generalized_indices: &[u64]) -> Vec<u64> {
    let mut all_helper_indices = HashSet::new();
    let mut all_path_indices = HashSet::new();

    for index in generalized_indices {
        all_helper_indices.extend(get_branch_indices(*index).iter());
        all_path_indices.extend(get_path_indices(*index).iter());
    }

    let mut all_branch_indices = all_helper_indices
        .difference(&all_path_indices)
        .cloned()
        .collect::<Vec<_>>();

    all_branch_indices.sort_by(|a: &u64, b: &u64| b.cmp(a)); // descending order
    all_branch_indices
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPTH: u64 = 3;

    #[test]
    /// See this [illustration](https://hackmd.io/_uploads/H1ZVOVille.png).
    fn test_get_helper_indices() {
        let leaf_indices = [2, 7];
        let generalized_indices = leaf_indices
            .iter()
            .map(|&index| generalized_index_from_leaf_index(index, DEPTH))
            .collect::<Vec<_>>();
        let helper_indices = get_helper_indices(&generalized_indices);
        assert_eq!(helper_indices.len(), 4);
        assert_eq!(helper_indices[0], 14);
        assert_eq!(helper_indices[1], 11);
        assert_eq!(helper_indices[2], 6);
        assert_eq!(helper_indices[3], 4);
    }
}

//! https://ethereum.github.io/consensus-specs/ssz/merkle-proofs

use alloy_primitives::B256;
use anyhow::ensure;

fn get_generalized_index_bit(index: u64, position: u64) -> bool {
    (index & (1 << position)) > 0
}

fn get_generalized_index_child(index: u64, right_side: bool) -> u64 {
    index * 2 + right_side as u64
}

fn get_subtree_index(generalized_index: u64) -> u64 {
    generalized_index % (1 << (generalized_index as f64).log2().floor() as u64)
}

pub fn merkle_tree(leaves: &[B256], depth: u64) -> anyhow::Result<Vec<B256>> {
    let num_of_leaves = leaves.len();
    let bottom_length = 1 << depth;
    ensure!(
        num_of_leaves <= bottom_length,
        "Number of leaves is greater than the bottom length (depth too small)"
    );

    let mut tree = vec![B256::ZERO; bottom_length];
    tree.extend(leaves);
    tree.extend(vec![B256::ZERO; bottom_length - num_of_leaves]);

    for i in (1..bottom_length).rev() {
        let left = tree[i * 2].as_slice();
        let right = tree[i * 2 + 1].as_slice();
        tree[i] = ethereum_hashing::hash32_concat(left, right).into();
    }

    Ok(tree)
}

pub fn generate_proof(tree: &[B256], index: u64, depth: u64) -> anyhow::Result<Vec<B256>> {
    let bottom_length = 1 << depth;
    ensure!(index < bottom_length, "Index out of bounds");

    let mut proof = vec![];
    let mut current_index = 1;
    let mut current_depth = depth;

    while current_depth > 0 {
        let (left_child_index, right_child_index) = (
            get_generalized_index_child(current_index, false),
            get_generalized_index_child(current_index, true),
        );

        if get_generalized_index_bit(index, current_depth - 1) {
            proof.push(tree[left_child_index as usize]);
            current_index = right_child_index;
        } else {
            proof.push(tree[right_child_index as usize]);
            current_index = left_child_index;
        }

        current_depth -= 1;
    }

    proof.reverse();

    Ok(proof)
}

pub fn is_valid_merkle_branch(
    leaf: B256,
    branch: &[B256],
    depth: u64,
    index: u64,
    root: B256,
) -> bool {
    let mut value = leaf;
    for i in 0..depth {
        if get_generalized_index_bit(index, i) {
            value =
                ethereum_hashing::hash32_concat(branch[i as usize].as_slice(), value.as_slice())
                    .into();
        } else {
            value =
                ethereum_hashing::hash32_concat(value.as_slice(), branch[i as usize].as_slice())
                    .into();
        }
    }
    value == root
}

pub fn is_valid_normalized_merkle_branch(
    leaf: B256,
    branch: &[B256],
    generalized_index: u64,
    root: B256,
) -> bool {
    let depth = (generalized_index as f64).log2().floor() as u64;
    let index = get_subtree_index(generalized_index);
    let num_extra = branch.len() - depth as usize;
    for node in branch[..num_extra].iter() {
        if *node != B256::ZERO {
            return false;
        }
    }
    is_valid_merkle_branch(leaf, &branch[num_extra..], depth, index, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree() {
        let leaves = vec![
            B256::from_slice(&[0xAA; 32]),
            B256::from_slice(&[0xBB; 32]),
            B256::from_slice(&[0xCC; 32]),
            B256::from_slice(&[0xDD; 32]),
        ];

        let depth = (leaves.len() as f64).log2().floor() as u64;

        let node_2: B256 =
            ethereum_hashing::hash32_concat(leaves[0].as_slice(), leaves[1].as_slice()).into();
        let node_3: B256 =
            ethereum_hashing::hash32_concat(leaves[2].as_slice(), leaves[3].as_slice()).into();

        let root: B256 =
            ethereum_hashing::hash32_concat(node_2.as_slice(), node_3.as_slice()).into();

        let tree = merkle_tree(&leaves, depth).unwrap();

        assert_eq!(tree[1], root);

        let proof_0 = generate_proof(&tree, 0, depth).unwrap();
        let proof_1 = generate_proof(&tree, 1, depth).unwrap();
        let proof_2 = generate_proof(&tree, 2, depth).unwrap();
        let proof_3 = generate_proof(&tree, 3, depth).unwrap();

        assert!(is_valid_merkle_branch(leaves[0], &proof_0, depth, 0, root));
        assert!(is_valid_merkle_branch(leaves[1], &proof_1, depth, 1, root));
        assert!(is_valid_merkle_branch(leaves[2], &proof_2, depth, 2, root));
        assert!(is_valid_merkle_branch(leaves[3], &proof_3, depth, 3, root));

        assert!(is_valid_normalized_merkle_branch(
            leaves[0],
            &proof_0,
            2 * depth,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[1],
            &proof_1,
            2 * depth + 1,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[2],
            &proof_2,
            2 * depth + 2,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[3],
            &proof_3,
            2 * depth + 3,
            root
        ));
    }
}

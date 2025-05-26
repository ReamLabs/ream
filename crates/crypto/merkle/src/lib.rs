//! https://ethereum.github.io/consensus-specs/ssz/merkle-proofs

pub mod multiproof;

use alloy_primitives::B256;
use anyhow::ensure;

mod hash;
mod index;

use hash::hash_concat;
use index::{generalized_index_child, get_generalized_index_bit, get_subtree_index};

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
        tree[i] = hash_concat(left, right);
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
            generalized_index_child(current_index, false),
            generalized_index_child(current_index, true),
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
            value = hash_concat(branch[i as usize].as_slice(), value.as_slice());
        } else {
            value = hash_concat(value.as_slice(), branch[i as usize].as_slice());
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

        let depth = (leaves.len() as f64).log2().ceil() as u64;

        let node_2 = hash_concat(leaves[0].as_slice(), leaves[1].as_slice());
        let node_3 = hash_concat(leaves[2].as_slice(), leaves[3].as_slice());

        let root = hash_concat(node_2.as_slice(), node_3.as_slice());

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
            1 << depth,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[1],
            &proof_1,
            (1 << depth) + 1,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[2],
            &proof_2,
            (1 << depth) + 2,
            root
        ));
        assert!(is_valid_normalized_merkle_branch(
            leaves[3],
            &proof_3,
            (1 << depth) + 3,
            root
        ));
    }
}
